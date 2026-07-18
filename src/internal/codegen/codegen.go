package codegen

import (
	"fmt"
	"regexp"
	"strings"

	"hyperlang.dev/hyperc/internal/ast"
)

// dopasowuje `self` jako całe słowo (nie jako część innego identyfikatora,
// np. `myself` czy `self2` ma pozostać nietknięte)
var selfWordRe = regexp.MustCompile(`\bself\b`)

type Generator struct {
	sb     strings.Builder
	indent int

	// tabela symboli zbierana w pierwszym przebiegu (potrzebna np. by `match`
	// wiedział jakie pola ma wariant enuma, a literał struktury znał kolejność
	// pól konstruktora)
	enumVariantFields map[string][]string // "Shape.Circle" -> ["radius"]
	structFieldOrder  map[string][]string // "Point" -> ["x","y"]
}

func New() *Generator {
	return &Generator{
		enumVariantFields: map[string][]string{},
		structFieldOrder:  map[string][]string{},
	}
}

func (g *Generator) write(s string)     { g.sb.WriteString(s) }
func (g *Generator) writeIndent()       { g.sb.WriteString(strings.Repeat("  ", g.indent)) }
func (g *Generator) writeLine(s string) { g.writeIndent(); g.write(s); g.write("\n") }

// Generate zwraca gotowy kod JavaScript dla całego programu.
func (g *Generator) Generate(prog *ast.Program) string {
	g.collectSymbols(prog.Statements)
	g.write("// wygenerowane przez hyperc - nie edytuj ręcznie\n")
	g.write("\"use strict\";\n\n")
	for _, stmt := range prog.Statements {
		g.genStmt(stmt)
	}
	return g.sb.String()
}

// ---------------------------------------------------------------------
// Zbieranie symboli (pierwszy przebieg) - potrzebne, bo match/struct-lit
// mogą odwoływać się do typów zdefiniowanych DALEJ w pliku.
// ---------------------------------------------------------------------

func (g *Generator) collectSymbols(stmts []ast.Stmt) {
	for _, s := range stmts {
		switch d := s.(type) {
		case *ast.StructDecl:
			var order []string
			for _, f := range d.Fields {
				order = append(order, f.Name)
			}
			g.structFieldOrder[d.Name] = order
		case *ast.EnumDecl:
			for _, v := range d.Variants {
				var fields []string
				for _, f := range v.Fields {
					fields = append(fields, f.Name)
				}
				g.enumVariantFields[d.Name+"."+v.Name] = fields
				g.enumVariantFields[v.Name] = fields // dostęp też po samej nazwie wariantu
			}
		case *ast.BlockStmt:
			g.collectSymbols(d.Statements)
		}
	}
}

// ---------------------------------------------------------------------
// Instrukcje
// ---------------------------------------------------------------------

func (g *Generator) genStmt(s ast.Stmt) {
	switch n := s.(type) {
	case *ast.ImportStmt:
		g.genImport(n)
	case *ast.LetStmt:
		g.genLet(n)
	case *ast.DestructureLetStmt:
		g.genDestructureLet(n)
	case *ast.TypeAliasStmt:
		// erasure: aliasy typów nie generują kodu JS
	case *ast.DeclareModuleStmt:
		// erasure: tylko sygnatury dla typecheckera, brak kodu JS
	case *ast.FnDecl:
		g.genFnDecl(n)
	case *ast.StructDecl:
		g.genStructDecl(n)
	case *ast.EnumDecl:
		g.genEnumDecl(n)
	case *ast.ImplDecl:
		g.genImplDecl(n)
	case *ast.TraitDecl:
		// erasure: trait to czysto strukturalny kontrakt typów
	case *ast.ExprStmt:
		g.writeIndent()
		g.genExpr(n.X)
		g.write(";\n")
	case *ast.ReturnStmt:
		g.writeIndent()
		g.write("return")
		if n.Value != nil {
			g.write(" ")
			g.genExpr(n.Value)
		}
		g.write(";\n")
	case *ast.IfStmt:
		g.genIf(n)
	case *ast.WhileStmt:
		g.writeIndent()
		g.write("while (")
		g.genExpr(n.Cond)
		g.write(") ")
		g.genBlockInline(n.Body)
	case *ast.ForInStmt:
		g.writeIndent()
		g.write(fmt.Sprintf("for (const %s of ", n.VarName))
		g.genExpr(n.Iterable)
		g.write(") ")
		g.genBlockInline(n.Body)
	case *ast.BreakStmt:
		g.writeLine("break;")
	case *ast.ContinueStmt:
		g.writeLine("continue;")
	case *ast.ThrowStmt:
		g.writeIndent()
		g.write("throw ")
		g.genExpr(n.Value)
		g.write(";\n")
	case *ast.TryStmt:
		g.genTry(n)
	case *ast.BlockStmt:
		g.genBlockInline(n)
	default:
		g.writeLine(fmt.Sprintf("/* nieobsłużona instrukcja: %T */", n))
	}
}

func (g *Generator) genTry(n *ast.TryStmt) {
	g.writeIndent()
	g.write("try ")
	g.genBlockInline(n.Try)
	if n.Catch != nil {
		s := g.sb.String()
		if strings.HasSuffix(s, "}\n") {
			trimmed := strings.TrimSuffix(s, "\n")
			g.sb.Reset()
			g.sb.WriteString(trimmed)
			g.write(" ")
		} else {
			g.writeIndent()
		}
		if n.CatchParam != "" {
			g.write(fmt.Sprintf("catch (%s) ", n.CatchParam))
		} else {
			g.write("catch ")
		}
		g.genBlockInline(n.Catch)
	}
	if n.Finally != nil {
		s := g.sb.String()
		if strings.HasSuffix(s, "}\n") {
			trimmed := strings.TrimSuffix(s, "\n")
			g.sb.Reset()
			g.sb.WriteString(trimmed)
			g.write(" ")
		} else {
			g.writeIndent()
		}
		g.write("finally ")
		g.genBlockInline(n.Finally)
	}
}

func (g *Generator) genImport(n *ast.ImportStmt) {
	g.writeIndent()
	path := fmt.Sprintf("%q", n.Path)
	named := formatNamedImports(n.Named)
	switch {
	case n.Default == "" && len(n.Named) == 0:
		g.write(fmt.Sprintf("import %s;\n", path))
	case n.Default != "" && len(n.Named) == 0:
		g.write(fmt.Sprintf("import %s from %s;\n", n.Default, path))
	case n.Default == "" && len(n.Named) > 0:
		g.write(fmt.Sprintf("import { %s } from %s;\n", named, path))
	default:
		g.write(fmt.Sprintf("import %s, { %s } from %s;\n", n.Default, named, path))
	}
}

func formatNamedImports(names []ast.ImportedName) string {
	parts := make([]string, len(names))
	for i, n := range names {
		if n.Alias != "" {
			parts[i] = fmt.Sprintf("%s as %s", n.Name, n.Alias)
		} else {
			parts[i] = n.Name
		}
	}
	return strings.Join(parts, ", ")
}

func (g *Generator) genLet(n *ast.LetStmt) {
	g.writeIndent()
	kw := "let"
	if n.Const || !n.Mutable {
		kw = "const" // domyślnie niemutowalne => const, jak w Rust
	}
	g.write(fmt.Sprintf("%s %s", kw, n.Name))
	if n.Value != nil {
		g.write(" = ")
		g.genExpr(n.Value)
	}
	g.write(";\n")
}

func (g *Generator) genDestructureLet(n *ast.DestructureLetStmt) {
	g.writeIndent()
	kw := "let"
	if n.Const || !n.Mutable {
		kw = "const"
	}
	parts := make([]string, len(n.Targets))
	if n.IsArray {
		for i, t := range n.Targets {
			if t.Rest {
				parts[i] = "..." + t.Name
			} else {
				parts[i] = t.Name
			}
		}
		g.write(fmt.Sprintf("%s [%s] = ", kw, strings.Join(parts, ", ")))
	} else {
		for i, t := range n.Targets {
			switch {
			case t.Rest:
				parts[i] = "..." + t.Name
			case t.Key == t.Name:
				parts[i] = t.Name
			default:
				parts[i] = fmt.Sprintf("%s: %s", t.Key, t.Name)
			}
		}
		g.write(fmt.Sprintf("%s { %s } = ", kw, strings.Join(parts, ", ")))
	}
	g.genExpr(n.Value)
	g.write(";\n")
}

func (g *Generator) genFnDecl(n *ast.FnDecl) {
	g.genJSDoc(n.Generics, n.Params, n.ReturnType)
	g.writeIndent()
	if n.IsAsync {
		g.write("async ")
	}
	g.write(fmt.Sprintf("function %s(%s) ", n.Name, g.paramList(n.Params)))
	g.genFnBody(n.Body, n.ExprBody)
}

// genJSDoc emituje blok /** @template T @param {...} @returns {...} */
// nad deklaracją. To jest ODPOWIEDŹ v0.1 na "generyki w codegenie": same
// generyki są (i powinny być - JS jest dynamicznie typowany, tak samo robi
// to `tsc`) usuwane w czasie transpilacji, bo w runtime nie mają żadnego
// znaczenia. Ale ich ŚLAD nie musi być zerowy - VSCode/inne edytory czytają
// JSDoc i dają prawdziwe podpowiedzi typów oraz sprawdzanie w wygenerowanym
// .js nawet bez dedykowanego LSP Hyper Lang, więc `hyperc` emituje je
// automatycznie dla każdej funkcji z generykami lub jawnymi typami.
func (g *Generator) genJSDoc(generics []string, params []ast.Param, returnType string) {
	if len(generics) == 0 && !anyParamTyped(params) && returnType == "" {
		return
	}
	g.writeIndent()
	g.write("/**\n")
	for _, gp := range generics {
		g.writeIndent()
		g.write(fmt.Sprintf(" * @template %s\n", gp))
	}
	for _, p := range params {
		if p.Name == "self" {
			continue
		}
		t := jsDocType(p.Type)
		g.writeIndent()
		g.write(fmt.Sprintf(" * @param {%s} %s\n", t, p.Name))
	}
	if returnType != "" {
		g.writeIndent()
		g.write(fmt.Sprintf(" * @returns {%s}\n", jsDocType(returnType)))
	}
	g.writeIndent()
	g.write(" */\n")
}

func anyParamTyped(params []ast.Param) bool {
	for _, p := range params {
		if p.Name != "self" && p.Type != "" {
			return true
		}
	}
	return false
}

// jsDocType tłumaczy adnotację typu Hyper na (przybliżony) typ JSDoc/Closure.
func jsDocType(t string) string {
	t = strings.TrimSpace(t)
	if t == "" {
		return "*"
	}
	optional := strings.HasSuffix(t, "?")
	t = strings.TrimSuffix(t, "?")
	array := strings.HasSuffix(t, "[]")
	t = strings.TrimSuffix(t, "[]")

	switch t {
	case "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64", "number":
		t = "number"
	case "string":
		t = "string"
	case "bool":
		t = "boolean"
	case "void":
		t = "void"
	case "any", "unknown":
		t = "*"
	}
	if array {
		t = t + "[]"
	}
	if optional {
		t = "?" + t
	}
	return t
}

func (g *Generator) genFnBody(body *ast.BlockStmt, exprBody ast.Expr) {
	if body != nil {
		g.genBlockInline(body)
		return
	}
	g.write("{\n")
	g.indent++
	g.writeIndent()
	g.write("return ")
	g.genExpr(exprBody)
	g.write(";\n")
	g.indent--
	g.writeIndent()
	g.write("}\n")
}

func (g *Generator) paramList(params []ast.Param) string {
	parts := make([]string, 0, len(params))
	for _, p := range params {
		if p.Name == "self" {
			continue // `self` => niejawny `this` w JS
		}
		s := p.Name
		if p.Rest {
			s = "..." + s
		}
		if p.Default != nil {
			s += " = " + g.exprToString(p.Default)
		}
		parts = append(parts, s)
	}
	return strings.Join(parts, ", ")
}

func (g *Generator) genBlockInline(b *ast.BlockStmt) {
	g.write("{\n")
	g.indent++
	for _, s := range b.Statements {
		g.genStmt(s)
	}
	g.indent--
	g.writeIndent()
	g.write("}\n")
}

func (g *Generator) genIf(n *ast.IfStmt) {
	g.writeIndent()
	g.write("if (")
	g.genExpr(n.Cond)
	g.write(") ")
	g.genBlockInline(n.Then)
	if n.Else != nil {
		g.writeIndent()
		g.sb.WriteString("") // no-op, zachowujemy wcięcie linii "else"
		// cofamy ostatni znak nowej linii, by doszyć "else" do "}"
		s := g.sb.String()
		if strings.HasSuffix(s, "}\n") {
			trimmed := strings.TrimSuffix(s, "\n")
			g.sb.Reset()
			g.sb.WriteString(trimmed)
			g.write(" else ")
		} else {
			g.write("else ")
		}
		switch e := n.Else.(type) {
		case *ast.BlockStmt:
			g.genBlockInline(e)
		case *ast.IfStmt:
			g.genIfNoIndent(e)
		}
	}
}

// warianty else-if nie powinny dublować wcięcia (doklejane po "else ")
func (g *Generator) genIfNoIndent(n *ast.IfStmt) {
	g.write("if (")
	g.genExpr(n.Cond)
	g.write(") ")
	g.genBlockInline(n.Then)
	if n.Else != nil {
		s := g.sb.String()
		if strings.HasSuffix(s, "}\n") {
			trimmed := strings.TrimSuffix(s, "\n")
			g.sb.Reset()
			g.sb.WriteString(trimmed)
			g.write(" else ")
		}
		switch e := n.Else.(type) {
		case *ast.BlockStmt:
			g.genBlockInline(e)
		case *ast.IfStmt:
			g.genIfNoIndent(e)
		}
	}
}

// ---------------------------------------------------------------------
// struct -> klasa JS
// ---------------------------------------------------------------------

func (g *Generator) genStructDecl(n *ast.StructDecl) {
	if len(n.Generics) > 0 {
		g.writeIndent()
		g.write("/**\n")
		for _, gp := range n.Generics {
			g.writeIndent()
			g.write(fmt.Sprintf(" * @template %s\n", gp))
		}
		g.writeIndent()
		g.write(" */\n")
	}
	g.writeIndent()
	g.write(fmt.Sprintf("class %s {\n", n.Name))
	g.indent++
	g.writeIndent()
	names := make([]string, len(n.Fields))
	for i, f := range n.Fields {
		names[i] = f.Name
	}
	g.write(fmt.Sprintf("constructor(%s) {\n", strings.Join(names, ", ")))
	g.indent++
	for _, f := range n.Fields {
		g.writeIndent()
		g.write(fmt.Sprintf("this.%s = %s", f.Name, f.Name))
		if f.Default != nil {
			g.write(" ?? ")
			g.genExpr(f.Default)
		}
		g.write(";\n")
	}
	g.indent--
	g.writeIndent()
	g.write("}\n")
	g.indent--
	g.writeIndent()
	g.write("}\n")
}

// ---------------------------------------------------------------------
// enum -> klasa z tagiem + statyczne fabryki wariantów
// ---------------------------------------------------------------------

func (g *Generator) genEnumDecl(n *ast.EnumDecl) {
	g.writeIndent()
	g.write(fmt.Sprintf("class %s {\n", n.Name))
	g.indent++
	g.writeLine("constructor(tag, values) {")
	g.indent++
	g.writeLine("this.tag = tag;")
	g.writeLine("Object.assign(this, values);")
	g.indent--
	g.writeLine("}")
	for _, v := range n.Variants {
		fnames := make([]string, len(v.Fields))
		for i, f := range v.Fields {
			fnames[i] = f.Name
		}
		g.writeIndent()
		g.write(fmt.Sprintf("static %s(%s) {\n", v.Name, strings.Join(fnames, ", ")))
		g.indent++
		g.writeIndent()
		g.write(fmt.Sprintf("return new %s(%q, { %s });\n", n.Name, v.Name, strings.Join(fnames, ", ")))
		g.indent--
		g.writeIndent()
		g.write("}\n")
	}
	g.indent--
	g.writeIndent()
	g.write("}\n")
}

// ---------------------------------------------------------------------
// impl -> metody dopięte do prototypu klasy
// ---------------------------------------------------------------------

func (g *Generator) genImplDecl(n *ast.ImplDecl) {
	for _, m := range n.Methods {
		g.genJSDoc(m.Generics, m.Params, m.ReturnType)
		g.writeIndent()
		asyncKw := ""
		if m.IsAsync {
			asyncKw = "async "
		}
		g.write(fmt.Sprintf("%s.prototype.%s = %sfunction(%s) ", n.Target, m.Name, asyncKw, g.paramList(m.Params)))
		g.genFnBody(m.Body, m.ExprBody)
	}
}

// ---------------------------------------------------------------------
// Wyrażenia
// ---------------------------------------------------------------------

func (g *Generator) exprToString(e ast.Expr) string {
	tmp := &Generator{enumVariantFields: g.enumVariantFields, structFieldOrder: g.structFieldOrder}
	tmp.genExpr(e)
	return tmp.sb.String()
}

func (g *Generator) genExpr(e ast.Expr) {
	switch n := e.(type) {
	case *ast.Ident:
		g.write(n.Name)
	case *ast.IntLit:
		g.write(n.Value)
	case *ast.FloatLit:
		g.write(n.Value)
	case *ast.StringLit:
		g.write(fmt.Sprintf("%q", n.Value))
	case *ast.TemplateLit:
		// przechodzi niemal 1:1 (identyczna składnia co JS template literal),
		// poza `self`, które wewnątrz `${...}` musi stać się `this`.
		g.write(selfWordRe.ReplaceAllString(n.Value, "this"))
	case *ast.BoolLit:
		if n.Value {
			g.write("true")
		} else {
			g.write("false")
		}
	case *ast.NullLit:
		g.write("null")
	case *ast.SelfExpr:
		g.write("this")
	case *ast.ArrayLit:
		g.write("[")
		for i, el := range n.Elements {
			if i > 0 {
				g.write(", ")
			}
			g.genExpr(el)
		}
		g.write("]")
	case *ast.ObjectLit:
		g.write("{")
		for i, k := range n.Order {
			if i > 0 {
				g.write(", ")
			}
			if _, isSpread := n.Values[k].(*ast.SpreadExpr); isSpread {
				g.genExpr(n.Values[k]) // "..." + wartość, bez nazwy klucza
			} else {
				g.write(fmt.Sprintf("%q: ", k))
				g.genExpr(n.Values[k])
			}
		}
		g.write("}")
	case *ast.SpreadExpr:
		g.write("...")
		g.genExpr(n.Value)
	case *ast.StructLit:
		g.genStructLit(n)
	case *ast.BinaryExpr:
		prec := binPrec(n.Op)
		g.genBinOperand(n.Left, prec, false)
		g.write(" " + jsOp(n.Op) + " ")
		g.genBinOperand(n.Right, prec, true)
	case *ast.UnaryExpr:
		g.write(n.Op)
		switch n.Operand.(type) {
		case *ast.BinaryExpr, *ast.TernaryExpr, *ast.NullishExpr, *ast.AssignExpr:
			g.write("(")
			g.genExpr(n.Operand)
			g.write(")")
		default:
			g.genExpr(n.Operand)
		}
	case *ast.NullishExpr:
		g.genExpr(n.Left)
		g.write(" ?? ")
		g.genExpr(n.Right)
	case *ast.TernaryExpr:
		g.write("(")
		g.genExpr(n.Cond)
		g.write(" ? ")
		g.genExpr(n.Then)
		g.write(" : ")
		g.genExpr(n.Else)
		g.write(")")
	case *ast.AssignExpr:
		g.genExpr(n.Target)
		g.write(" " + n.Op + " ")
		g.genExpr(n.Value)
	case *ast.CallExpr:
		g.genExpr(n.Callee)
		g.write("(")
		for i, a := range n.Args {
			if i > 0 {
				g.write(", ")
			}
			g.genExpr(a)
		}
		g.write(")")
	case *ast.NewExpr:
		g.write("new ")
		g.genExpr(n.Callee)
		g.write("(")
		for i, a := range n.Args {
			if i > 0 {
				g.write(", ")
			}
			g.genExpr(a)
		}
		g.write(")")
	case *ast.MemberExpr:
		g.genExpr(n.Object)
		if n.Optional {
			g.write("?.")
		} else {
			g.write(".")
		}
		g.write(n.Property)
	case *ast.IndexExpr:
		g.genExpr(n.Object)
		g.write("[")
		g.genExpr(n.Index)
		g.write("]")
	case *ast.AwaitExpr:
		g.write("await ")
		g.genExpr(n.Value)
	case *ast.ArrowFnExpr:
		g.genArrowFn(n)
	case *ast.MatchExpr:
		g.genMatch(n)
	default:
		g.write(fmt.Sprintf("/* nieobsłużone wyrażenie: %T */ null", n))
	}
}

// binPrec zwraca precedencję operatora binarnego (wyższa = wiąże mocniej),
// zgodną z tabelą w parserze i ze standardową precedencją JS. Używana do
// decydowania, kiedy podwyrażenie MUSI dostać nawiasy w wygenerowanym JS,
// żeby zachować to samo grupowanie co w oryginalnym kodzie Hyper.
func binPrec(op string) int {
	switch op {
	case "||":
		return 1
	case "&&":
		return 2
	case "==", "!=":
		return 3
	case "<", ">", "<=", ">=":
		return 4
	case "+", "-":
		return 5
	case "*", "/", "%":
		return 6
	}
	return 0
}

// genBinOperand emituje podwyrażenie operandu wyrażenia binarnego, dodając
// nawiasy tam, gdzie ich brak zmieniłby znaczenie: gdy podwyrażenie ma
// NIŻSZĄ precedencję niż operator nadrzędny, albo gdy jest to prawy operand
// o RÓWNEJ precedencji operatora nieprzemiennego (np. `a - (b - c)` musi
// zachować nawiasy, bo `a - b - c` znaczy coś innego).
func (g *Generator) genBinOperand(e ast.Expr, parentPrec int, isRight bool) {
	if be, ok := e.(*ast.BinaryExpr); ok {
		childPrec := binPrec(be.Op)
		if childPrec < parentPrec || (isRight && childPrec == parentPrec) {
			g.write("(")
			g.genExpr(e)
			g.write(")")
			return
		}
	}
	switch e.(type) {
	case *ast.TernaryExpr, *ast.NullishExpr, *ast.AssignExpr, *ast.ArrowFnExpr:
		g.write("(")
		g.genExpr(e)
		g.write(")")
	default:
		g.genExpr(e)
	}
}

func jsOp(op string) string {
	// większość operatorów jest identyczna w Hyper i JS
	return op
}

func (g *Generator) genStructLit(n *ast.StructLit) {
	order, known := g.structFieldOrder[n.Name]
	g.write(fmt.Sprintf("new %s(", n.Name))
	if known {
		for i, fname := range order {
			if i > 0 {
				g.write(", ")
			}
			if val, ok := n.Values[fname]; ok {
				g.genExpr(val)
			} else {
				g.write("undefined")
			}
		}
	} else {
		// nieznana struktura (np. zdefiniowana w innym pliku/module) - przekaż
		// jako obiekt pozycyjnie w kolejności zapisu w kodzie źródłowym
		for i, fname := range n.Order {
			if i > 0 {
				g.write(", ")
			}
			g.genExpr(n.Values[fname])
		}
	}
	g.write(")")
}

func (g *Generator) genArrowFn(n *ast.ArrowFnExpr) {
	if n.IsAsync {
		g.write("async ")
	}
	g.write("(" + g.paramList(n.Params) + ") => ")
	if n.Body != nil {
		g.genBlockInline(n.Body)
	} else {
		g.write("(")
		g.genExpr(n.ExprBody)
		g.write(")")
	}
}

// match subject { Circle(r) => expr, Rectangle(w,h) => expr, _ => expr }
// kompiluje się do natychmiast wywoływanej funkcji strzałkowej z if/else
// po polu `.tag`, z destrukturyzacją pól wariantu do zmiennych z wzorca.
func (g *Generator) genMatch(n *ast.MatchExpr) {
	g.write("(() => {\n")
	g.indent++
	g.writeIndent()
	g.write("const __subject = ")
	g.genExpr(n.Subject)
	g.write(";\n")

	for i, arm := range n.Arms {
		g.writeIndent()
		kw := "if"
		if i > 0 {
			kw = "else if"
		}
		if arm.Pattern.Wildcard {
			g.write("{\n")
		} else if arm.Pattern.Variant != "" && arm.Pattern.Bindings == nil && !g.isKnownVariant(arm.Pattern.Variant) {
			// zwykły identyfikator jako wzorzec bindujący całość (rzadkie, fallback)
			g.write(fmt.Sprintf("%s (true) {\n", kw))
		} else if arm.Pattern.Variant != "" {
			g.write(fmt.Sprintf("%s (__subject.tag === %q) {\n", kw, arm.Pattern.Variant))
		} else {
			g.write(fmt.Sprintf("%s (__subject === ", kw))
			g.genExpr(arm.Pattern.Literal)
			g.write(") {\n")
		}
		g.indent++
		if len(arm.Pattern.Bindings) > 0 {
			fields := g.enumVariantFields[arm.Pattern.Variant]
			g.writeIndent()
			g.write("const { ")
			parts := make([]string, len(arm.Pattern.Bindings))
			for idx, bind := range arm.Pattern.Bindings {
				fname := bind
				if idx < len(fields) {
					fname = fields[idx]
				}
				if fname == bind {
					parts[idx] = bind
				} else {
					parts[idx] = fmt.Sprintf("%s: %s", fname, bind)
				}
			}
			g.write(strings.Join(parts, ", "))
			g.write(" } = __subject;\n")
		}
		g.writeIndent()
		g.write("return ")
		g.genMatchArmBody(arm.Body)
		g.write(";\n")
		g.indent--
		g.writeIndent()
		g.write("}\n")
	}
	g.writeLine("throw new Error(\"hyper: niewyczerpujący match\");")
	g.indent--
	g.writeIndent()
	g.write("})()")
}

func (g *Generator) genMatchArmBody(body ast.Expr) {
	if fn, ok := body.(*ast.ArrowFnExpr); ok && fn.Params == nil && fn.Body != nil {
		// ciało blokowe ramienia match (opakowane parserem jako ArrowFnExpr)
		// -> wywołujemy jako IIFE, by pozostać wyrażeniem
		g.write("(() => ")
		g.genBlockInline(fn.Body)
		g.write(")()")
		return
	}
	g.genExpr(body)
}

func (g *Generator) isKnownVariant(name string) bool {
	_, ok := g.enumVariantFields[name]
	return ok
}
