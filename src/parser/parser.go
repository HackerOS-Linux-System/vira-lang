package parser

import (
	"fmt"
	"strings"

	"hyperlang.dev/hyperc/internal/ast"
	"hyperlang.dev/hyperc/internal/lexer"
)

type Parser struct {
	l *lexer.Lexer

	cur  lexer.Token
	peek lexer.Token

	errors []string

	// noStructLit > 0 oznacza, że jesteśmy w pozycji, w której `{` po
	// identyfikatorze rozpoczyna BLOK (warunek if/while, subject match,
	// iterable w for-in), a nie literał struktury. Bez tego `if cond {`
	// próbowałby sparsować `cond { ... }` jako Cond{...}.
	noStructLit int
}

func New(l *lexer.Lexer) *Parser {
	p := &Parser{l: l}
	p.next()
	p.next()
	return p
}

func (p *Parser) Errors() []string { return p.errors }

func (p *Parser) next() {
	p.cur = p.peek
	p.peek = p.l.NextToken()
}

func (p *Parser) errorf(format string, args ...interface{}) {
	msg := fmt.Sprintf(format, args...)
	p.errors = append(p.errors, fmt.Sprintf("linia %d:%d: %s", p.cur.Line, p.cur.Col, msg))
}

func (p *Parser) curIs(tt lexer.TokenType) bool  { return p.cur.Type == tt }
func (p *Parser) peekIs(tt lexer.TokenType) bool { return p.peek.Type == tt }

func (p *Parser) expect(tt lexer.TokenType) lexer.Token {
	tok := p.cur
	if !p.curIs(tt) {
		p.errorf("oczekiwano %s, otrzymano %q (%s)", tt, p.cur.Literal, p.cur.Type)
		// UWAGA: mimo błędu i tak przesuwamy się o token, żeby parser nigdy
		// nie utknął w nieskończonej pętli próbując wciąż to samo dopasować.
		if !p.curIs(lexer.EOF) {
			p.next()
		}
		return tok
	}
	p.next()
	return tok
}

func (p *Parser) skipSemi() {
	for p.curIs(lexer.SEMI) {
		p.next()
	}
}

// ---------------------------------------------------------------------
// Program
// ---------------------------------------------------------------------

func (p *Parser) ParseProgram() *ast.Program {
	prog := &ast.Program{}
	for !p.curIs(lexer.EOF) {
		if p.curIs(lexer.SEMI) {
			p.next()
			continue
		}
		stmt := p.parseTopLevelStmt()
		if stmt != nil {
			prog.Statements = append(prog.Statements, stmt)
		}
		p.skipSemi()
	}
	return prog
}

func (p *Parser) parseTopLevelStmt() ast.Stmt {
	pub := false
	if p.curIs(lexer.PUB) {
		pub = true
		p.next()
	}
	switch p.cur.Type {
	case lexer.IMPORT:
		return p.parseImport()
	case lexer.LET, lexer.CONST:
		return p.parseLet()
	case lexer.FN:
		return p.parseFn(false, pub)
	case lexer.ASYNC:
		p.next()
		return p.parseFn(true, pub)
	case lexer.STRUCT:
		return p.parseStruct(pub)
	case lexer.ENUM:
		return p.parseEnum(pub)
	case lexer.IMPL:
		return p.parseImpl()
	case lexer.TRAIT:
		return p.parseTrait()
	case lexer.TYPE:
		return p.parseTypeAlias()
	case lexer.DECLARE:
		return p.parseDeclare()
	default:
		return p.parseStatement()
	}
}

func (p *Parser) parseStatement() ast.Stmt {
	switch p.cur.Type {
	case lexer.LET, lexer.CONST:
		return p.parseLet()
	case lexer.RETURN:
		return p.parseReturn()
	case lexer.IF:
		return p.parseIf()
	case lexer.WHILE:
		return p.parseWhile()
	case lexer.FOR:
		return p.parseForIn()
	case lexer.BREAK:
		p.next()
		return &ast.BreakStmt{}
	case lexer.CONTINUE:
		p.next()
		return &ast.ContinueStmt{}
	case lexer.FN:
		return p.parseFn(false, false)
	case lexer.ASYNC:
		p.next()
		return p.parseFn(true, false)
	case lexer.LBRACE:
		return p.parseBlock()
	default:
		expr := p.parseExpression(LOWEST)
		return &ast.ExprStmt{X: expr}
	}
}

// ---------------------------------------------------------------------
// import { a, b } from "mod"  /  import def from "mod"  /  import "mod"
// ---------------------------------------------------------------------

func (p *Parser) parseImport() ast.Stmt {
	p.next() // 'import'
	imp := &ast.ImportStmt{}

	if p.curIs(lexer.STRING) {
		imp.Path = p.cur.Literal
		p.next()
		return imp
	}

	if p.curIs(lexer.LBRACE) {
		p.next()
		for !p.curIs(lexer.RBRACE) && !p.curIs(lexer.EOF) {
			imp.Named = append(imp.Named, p.expect(lexer.IDENT).Literal)
			if p.curIs(lexer.COMMA) {
				p.next()
			}
		}
		p.expect(lexer.RBRACE)
	} else if p.curIs(lexer.IDENT) {
		imp.Default = p.cur.Literal
		p.next()
		if p.curIs(lexer.COMMA) {
			p.next()
			p.expect(lexer.LBRACE)
			for !p.curIs(lexer.RBRACE) && !p.curIs(lexer.EOF) {
				imp.Named = append(imp.Named, p.expect(lexer.IDENT).Literal)
				if p.curIs(lexer.COMMA) {
					p.next()
				}
			}
			p.expect(lexer.RBRACE)
		}
	}

	p.expect(lexer.FROM)
	imp.Path = p.expect(lexer.STRING).Literal
	return imp
}

// ---------------------------------------------------------------------
// let / const
// ---------------------------------------------------------------------

func (p *Parser) parseLet() ast.Stmt {
	isConst := p.curIs(lexer.CONST)
	p.next() // let/const

	mutable := false
	if p.curIs(lexer.MUT) {
		mutable = true
		p.next()
	}

	name := p.expect(lexer.IDENT).Literal

	typ := ""
	if p.curIs(lexer.COLON) {
		p.next()
		typ = p.parseTypeString()
	}

	var value ast.Expr
	if p.curIs(lexer.ASSIGN) {
		p.next()
		value = p.parseExpression(LOWEST)
	}

	return &ast.LetStmt{Name: name, Type: typ, Mutable: mutable, Const: isConst, Value: value}
}

func (p *Parser) parseTypeAlias() ast.Stmt {
	p.next() // 'type'
	name := p.expect(lexer.IDENT).Literal
	var generics []string
	if p.curIs(lexer.LT) {
		generics = p.parseGenericParams()
	}
	p.expect(lexer.ASSIGN)
	typ := p.parseTypeString()
	return &ast.TypeAliasStmt{Name: name, Params: generics, Type: typ}
}

func (p *Parser) parseDeclare() ast.Stmt {
	p.next() // 'declare'
	p.expect(lexer.MODULE)
	path := p.expect(lexer.STRING).Literal
	p.expect(lexer.LBRACE)
	// Ciało `declare module` to same sygnatury (bez implementacji) - parsujemy
	// pobieżnie i odrzucamy, bo erasure: żadnego kodu JS stąd nie generujemy.
	depth := 1
	for depth > 0 && !p.curIs(lexer.EOF) {
		if p.curIs(lexer.LBRACE) {
			depth++
		} else if p.curIs(lexer.RBRACE) {
			depth--
			if depth == 0 {
				break
			}
		}
		p.next()
	}
	p.expect(lexer.RBRACE)
	return &ast.DeclareModuleStmt{Path: path}
}

// ---------------------------------------------------------------------
// fn name<T>(a: T, b: i32 = 0) -> T { ... }   albo   fn f() => expr
// ---------------------------------------------------------------------

func (p *Parser) parseFn(isAsync bool, pub bool) *ast.FnDecl {
	p.expect(lexer.FN)
	name := p.expect(lexer.IDENT).Literal

	var generics []string
	if p.curIs(lexer.LT) {
		generics = p.parseGenericParams()
	}

	p.expect(lexer.LPAREN)
	params := p.parseParamList()
	p.expect(lexer.RPAREN)

	retType := ""
	if p.curIs(lexer.ARROW) {
		p.next()
		retType = p.parseTypeString()
	}

	fn := &ast.FnDecl{Name: name, Generics: generics, Params: params, ReturnType: retType, IsAsync: isAsync, Pub: pub}

	if p.curIs(lexer.FATARROW) {
		p.next()
		fn.ExprBody = p.parseExpression(LOWEST)
	} else {
		fn.Body = p.parseBlock()
	}
	return fn
}

func (p *Parser) parseGenericParams() []string {
	p.expect(lexer.LT)
	var out []string
	for !p.curIs(lexer.GT) && !p.curIs(lexer.EOF) {
		out = append(out, p.expect(lexer.IDENT).Literal)
		// ograniczenia typu (T: SomeTrait) są parsowane i odrzucane (erasure)
		if p.curIs(lexer.COLON) {
			p.next()
			p.parseTypeString()
		}
		if p.curIs(lexer.COMMA) {
			p.next()
		}
	}
	p.expect(lexer.GT)
	return out
}

func (p *Parser) parseParamList() []ast.Param {
	var params []ast.Param
	for !p.curIs(lexer.RPAREN) && !p.curIs(lexer.EOF) {
		param := ast.Param{}
		if p.curIs(lexer.SPREAD) {
			param.Rest = true
			p.next()
		}
		if p.curIs(lexer.SELF) {
			param.Name = "self"
			p.next()
		} else {
			param.Name = p.expect(lexer.IDENT).Literal
			if p.curIs(lexer.COLON) {
				p.next()
				param.Type = p.parseTypeString()
			}
			if p.curIs(lexer.ASSIGN) {
				p.next()
				param.Default = p.parseExpression(LOWEST)
			}
		}
		params = append(params, param)
		if p.curIs(lexer.COMMA) {
			p.next()
		}
	}
	return params
}

// parseTypeString konsumuje adnotację typu i zwraca jej tekstową reprezentację.
// hyperc v0.1 stosuje erasure typów (jak wczesne wersje TS-to-JS) - typy służą
// wyłącznie do (przyszłej) analizy statycznej, nie trafiają do wyjścia JS.
func (p *Parser) parseTypeString() string {
	var sb strings.Builder
	depth := 0
	for {
		switch p.cur.Type {
		case lexer.LT, lexer.LPAREN, lexer.LBRACKET:
			depth++
		case lexer.GT, lexer.RPAREN, lexer.RBRACKET:
			if depth == 0 {
				return strings.TrimSpace(sb.String())
			}
			depth--
		case lexer.COMMA, lexer.ASSIGN, lexer.LBRACE, lexer.SEMI, lexer.FATARROW:
			if depth == 0 {
				return strings.TrimSpace(sb.String())
			}
		case lexer.EOF:
			return strings.TrimSpace(sb.String())
		}
		sb.WriteString(p.cur.Literal)
		p.next()
		// heurystyka odstępu: nie doklejaj spacji po `?` `[` `<` itp.
		if p.curIs(lexer.RPAREN) || p.curIs(lexer.COMMA) {
			continue
		}
	}
}

// ---------------------------------------------------------------------
// struct
// ---------------------------------------------------------------------

func (p *Parser) parseStruct(pub bool) ast.Stmt {
	p.expect(lexer.STRUCT)
	name := p.expect(lexer.IDENT).Literal
	var generics []string
	if p.curIs(lexer.LT) {
		generics = p.parseGenericParams()
	}
	p.expect(lexer.LBRACE)
	var fields []ast.StructField
	for !p.curIs(lexer.RBRACE) && !p.curIs(lexer.EOF) {
		if p.curIs(lexer.PUB) {
			p.next()
		}
		fname := p.expect(lexer.IDENT).Literal
		p.expect(lexer.COLON)
		ftype := p.parseTypeString()
		f := ast.StructField{Name: fname, Type: ftype}
		if p.curIs(lexer.ASSIGN) {
			p.next()
			f.Default = p.parseExpression(LOWEST)
		}
		fields = append(fields, f)
		if p.curIs(lexer.COMMA) {
			p.next()
		}
	}
	p.expect(lexer.RBRACE)
	return &ast.StructDecl{Name: name, Generics: generics, Fields: fields, Pub: pub}
}

// ---------------------------------------------------------------------
// enum (algebraiczne typy danych)
// ---------------------------------------------------------------------

func (p *Parser) parseEnum(pub bool) ast.Stmt {
	p.expect(lexer.ENUM)
	name := p.expect(lexer.IDENT).Literal
	var generics []string
	if p.curIs(lexer.LT) {
		generics = p.parseGenericParams()
	}
	p.expect(lexer.LBRACE)
	var variants []ast.EnumVariant
	for !p.curIs(lexer.RBRACE) && !p.curIs(lexer.EOF) {
		vname := p.expect(lexer.IDENT).Literal
		variant := ast.EnumVariant{Name: vname}
		if p.curIs(lexer.LPAREN) {
			p.next()
			for !p.curIs(lexer.RPAREN) && !p.curIs(lexer.EOF) {
				fname := p.expect(lexer.IDENT).Literal
				ftype := ""
				if p.curIs(lexer.COLON) {
					p.next()
					ftype = p.parseTypeString()
				}
				variant.Fields = append(variant.Fields, ast.Param{Name: fname, Type: ftype})
				if p.curIs(lexer.COMMA) {
					p.next()
				}
			}
			p.expect(lexer.RPAREN)
		}
		variants = append(variants, variant)
		if p.curIs(lexer.COMMA) {
			p.next()
		}
	}
	p.expect(lexer.RBRACE)
	return &ast.EnumDecl{Name: name, Generics: generics, Variants: variants, Pub: pub}
}

// ---------------------------------------------------------------------
// impl Target { ... }   /   impl Trait for Target { ... }
// ---------------------------------------------------------------------

func (p *Parser) parseImpl() ast.Stmt {
	p.expect(lexer.IMPL)
	first := p.expect(lexer.IDENT).Literal
	impl := &ast.ImplDecl{}
	if p.curIs(lexer.FOR) {
		p.next()
		impl.Trait = first
		impl.Target = p.expect(lexer.IDENT).Literal
	} else {
		impl.Target = first
	}
	p.expect(lexer.LBRACE)
	for !p.curIs(lexer.RBRACE) && !p.curIs(lexer.EOF) {
		pub := false
		if p.curIs(lexer.PUB) {
			pub = true
			p.next()
		}
		isAsync := false
		if p.curIs(lexer.ASYNC) {
			isAsync = true
			p.next()
		}
		method := p.parseFn(isAsync, pub)
		impl.Methods = append(impl.Methods, method)
	}
	p.expect(lexer.RBRACE)
	return impl
}

func (p *Parser) parseTrait() ast.Stmt {
	p.expect(lexer.TRAIT)
	name := p.expect(lexer.IDENT).Literal
	if p.curIs(lexer.LT) {
		p.parseGenericParams()
	}
	p.expect(lexer.LBRACE)
	var sigs []ast.Param
	for !p.curIs(lexer.RBRACE) && !p.curIs(lexer.EOF) {
		p.expect(lexer.FN)
		mname := p.expect(lexer.IDENT).Literal
		if p.curIs(lexer.LT) {
			p.parseGenericParams()
		}
		p.expect(lexer.LPAREN)
		p.parseParamList()
		p.expect(lexer.RPAREN)
		if p.curIs(lexer.ARROW) {
			p.next()
			p.parseTypeString()
		}
		sigs = append(sigs, ast.Param{Name: mname})
		p.skipSemi()
	}
	p.expect(lexer.RBRACE)
	return &ast.TraitDecl{Name: name, Methods: sigs}
}

// ---------------------------------------------------------------------
// bloki i instrukcje sterujące
// ---------------------------------------------------------------------

func (p *Parser) parseBlock() *ast.BlockStmt {
	p.expect(lexer.LBRACE)
	block := &ast.BlockStmt{}
	for !p.curIs(lexer.RBRACE) && !p.curIs(lexer.EOF) {
		if p.curIs(lexer.SEMI) {
			p.next()
			continue
		}
		stmt := p.parseTopLevelStmtInBlock()
		block.Statements = append(block.Statements, stmt)
		p.skipSemi()
	}
	p.expect(lexer.RBRACE)
	return block
}

// wewnątrz bloków dopuszczamy też lokalne struct/enum/fn (przydatne w dużych projektach)
func (p *Parser) parseTopLevelStmtInBlock() ast.Stmt {
	switch p.cur.Type {
	case lexer.STRUCT:
		return p.parseStruct(false)
	case lexer.ENUM:
		return p.parseEnum(false)
	case lexer.IMPL:
		return p.parseImpl()
	default:
		return p.parseStatement()
	}
}

func (p *Parser) parseReturn() ast.Stmt {
	p.next()
	if p.curIs(lexer.SEMI) || p.curIs(lexer.RBRACE) {
		return &ast.ReturnStmt{}
	}
	return &ast.ReturnStmt{Value: p.parseExpression(LOWEST)}
}

func (p *Parser) parseIf() ast.Stmt {
	p.expect(lexer.IF)
	cond := p.parseExprNoStruct(LOWEST)
	then := p.parseBlock()
	stmt := &ast.IfStmt{Cond: cond, Then: then}
	if p.curIs(lexer.ELSE) {
		p.next()
		if p.curIs(lexer.IF) {
			stmt.Else = p.parseIf()
		} else {
			stmt.Else = p.parseBlock()
		}
	}
	return stmt
}

func (p *Parser) parseWhile() ast.Stmt {
	p.expect(lexer.WHILE)
	cond := p.parseExprNoStruct(LOWEST)
	body := p.parseBlock()
	return &ast.WhileStmt{Cond: cond, Body: body}
}

func (p *Parser) parseForIn() ast.Stmt {
	p.expect(lexer.FOR)
	varName := p.expect(lexer.IDENT).Literal
	p.expect(lexer.IN)
	iterable := p.parseExprNoStruct(LOWEST)
	body := p.parseBlock()
	return &ast.ForInStmt{VarName: varName, Iterable: iterable, Body: body}
}

// match subject { Pattern(x, y) => expr, _ => expr }
func (p *Parser) parseMatch() ast.Expr {
	p.expect(lexer.MATCH)
	subject := p.parseExprNoStruct(LOWEST)
	p.expect(lexer.LBRACE)
	m := &ast.MatchExpr{Subject: subject}
	for !p.curIs(lexer.RBRACE) && !p.curIs(lexer.EOF) {
		pat := p.parsePattern()
		p.expect(lexer.FATARROW)
		var body ast.Expr
		if p.curIs(lexer.LBRACE) {
			block := p.parseBlock()
			body = blockToExpr(block)
		} else {
			body = p.parseExpression(LOWEST)
		}
		m.Arms = append(m.Arms, ast.MatchArm{Pattern: pat, Body: body})
		if p.curIs(lexer.COMMA) {
			p.next()
		}
	}
	p.expect(lexer.RBRACE)
	return m
}

// blockToExpr opakowuje blok instrukcji jako IIFE, by mógł być użyty jako Expr
func blockToExpr(b *ast.BlockStmt) ast.Expr {
	return &ast.ArrowFnExpr{Params: nil, Body: b}
}

func (p *Parser) parsePattern() *ast.Pattern {
	if p.curIs(lexer.IDENT) && p.cur.Literal == "_" {
		p.next()
		return &ast.Pattern{Wildcard: true}
	}
	if p.curIs(lexer.IDENT) {
		name := p.cur.Literal
		p.next()
		pat := &ast.Pattern{Variant: name}
		if p.curIs(lexer.LPAREN) {
			p.next()
			for !p.curIs(lexer.RPAREN) && !p.curIs(lexer.EOF) {
				pat.Bindings = append(pat.Bindings, p.expect(lexer.IDENT).Literal)
				if p.curIs(lexer.COMMA) {
					p.next()
				}
			}
			p.expect(lexer.RPAREN)
		}
		return pat
	}
	// wzorzec literałowy
	lit := p.parseExpression(LOWEST)
	return &ast.Pattern{Literal: lit}
}

// ---------------------------------------------------------------------
// Wyrażenia: precedence climbing
// ---------------------------------------------------------------------

const (
	LOWEST = iota
	ASSIGNP
	NULLISHP
	ORP
	ANDP
	EQUALS
	RELATIONAL
	SUM
	PRODUCT
	UNARY
	POSTFIX
)

var precedences = map[lexer.TokenType]int{
	lexer.ASSIGN: ASSIGNP, lexer.PLUSEQ: ASSIGNP, lexer.MINUSEQ: ASSIGNP,
	lexer.STAREQ: ASSIGNP, lexer.SLASHEQ: ASSIGNP,
	lexer.NULLISH: NULLISHP,
	lexer.OROR:    ORP,
	lexer.ANDAND:  ANDP,
	lexer.EQ:      EQUALS, lexer.NEQ: EQUALS,
	lexer.LT: RELATIONAL, lexer.GT: RELATIONAL, lexer.LE: RELATIONAL, lexer.GE: RELATIONAL,
	lexer.PLUS: SUM, lexer.MINUS: SUM,
	lexer.STAR: PRODUCT, lexer.SLASH: PRODUCT, lexer.PERCENT: PRODUCT,
	lexer.LPAREN: POSTFIX, lexer.DOT: POSTFIX, lexer.QDOT: POSTFIX, lexer.LBRACKET: POSTFIX,
}

func (p *Parser) peekPrecedence() int {
	if pr, ok := precedences[p.peek.Type]; ok {
		return pr
	}
	return LOWEST
}

// parseExprNoStruct parsuje wyrażenie w kontekście, w którym `{` po
// identyfikatorze NIE może rozpoczynać literału struktury (bo w rzeczywistości
// rozpoczyna blok kodu: if/while/match/for).
func (p *Parser) parseExprNoStruct(minPrec int) ast.Expr {
	p.noStructLit++
	e := p.parseExpression(minPrec)
	p.noStructLit--
	return e
}

func (p *Parser) parseExpression(minPrec int) ast.Expr {
	left := p.parseUnary()

	for {
		pr, ok := precedences[p.cur.Type]
		if !ok || pr < minPrec {
			break
		}
		switch p.cur.Type {
		case lexer.LPAREN:
			left = p.parseCall(left)
		case lexer.DOT, lexer.QDOT:
			left = p.parseMember(left)
		case lexer.LBRACKET:
			left = p.parseIndex(left)
		case lexer.ASSIGN, lexer.PLUSEQ, lexer.MINUSEQ, lexer.STAREQ, lexer.SLASHEQ:
			op := p.cur.Literal
			p.next()
			value := p.parseExpression(ASSIGNP)
			left = &ast.AssignExpr{Target: left, Op: op, Value: value}
		case lexer.NULLISH:
			p.next()
			right := p.parseExpression(NULLISHP + 1)
			left = &ast.NullishExpr{Left: left, Right: right}
		default:
			op := p.cur.Literal
			p.next()
			right := p.parseExpression(pr + 1)
			left = &ast.BinaryExpr{Op: op, Left: left, Right: right}
		}
	}
	return left
}

func (p *Parser) parseUnary() ast.Expr {
	switch p.cur.Type {
	case lexer.BANG, lexer.MINUS, lexer.PLUS:
		op := p.cur.Literal
		p.next()
		operand := p.parseUnary()
		return &ast.UnaryExpr{Op: op, Operand: operand}
	case lexer.AWAIT:
		p.next()
		val := p.parseUnary()
		return &ast.AwaitExpr{Value: val}
	default:
		return p.parsePrimary()
	}
}

func (p *Parser) parseCall(callee ast.Expr) ast.Expr {
	p.expect(lexer.LPAREN)
	var args []ast.Expr
	for !p.curIs(lexer.RPAREN) && !p.curIs(lexer.EOF) {
		args = append(args, p.parseExpression(ASSIGNP+1))
		if p.curIs(lexer.COMMA) {
			p.next()
		}
	}
	p.expect(lexer.RPAREN)
	return &ast.CallExpr{Callee: callee, Args: args}
}

func (p *Parser) parseMember(obj ast.Expr) ast.Expr {
	optional := p.curIs(lexer.QDOT)
	p.next() // . albo ?.
	prop := p.expect(lexer.IDENT).Literal
	return &ast.MemberExpr{Object: obj, Property: prop, Optional: optional}
}

func (p *Parser) parseIndex(obj ast.Expr) ast.Expr {
	p.expect(lexer.LBRACKET)
	idx := p.parseExpression(LOWEST)
	p.expect(lexer.RBRACKET)
	return &ast.IndexExpr{Object: obj, Index: idx}
}

func (p *Parser) parsePrimary() ast.Expr {
	switch p.cur.Type {
	case lexer.INT:
		v := p.cur.Literal
		p.next()
		return &ast.IntLit{Value: v}
	case lexer.FLOAT:
		v := p.cur.Literal
		p.next()
		return &ast.FloatLit{Value: v}
	case lexer.STRING:
		v := p.cur.Literal
		p.next()
		return &ast.StringLit{Value: v}
	case lexer.TEMPLATE:
		v := p.cur.Literal
		p.next()
		return &ast.TemplateLit{Value: v}
	case lexer.TRUE:
		p.next()
		return &ast.BoolLit{Value: true}
	case lexer.FALSE:
		p.next()
		return &ast.BoolLit{Value: false}
	case lexer.NULLKW:
		p.next()
		return &ast.NullLit{}
	case lexer.SELF:
		p.next()
		return &ast.SelfExpr{}
	case lexer.IDENT:
		name := p.cur.Literal
		p.next()
		if p.curIs(lexer.LBRACE) && p.canBeStructLit() {
			return p.parseStructLit(name)
		}
		return &ast.Ident{Name: name}
	case lexer.NEW:
		p.next()
		callee := p.parseExpression(POSTFIX)
		if ce, ok := callee.(*ast.CallExpr); ok {
			return &ast.NewExpr{Callee: ce.Callee, Args: ce.Args}
		}
		return &ast.NewExpr{Callee: callee}
	case lexer.LPAREN:
		return p.parseParenOrArrow()
	case lexer.LBRACKET:
		return p.parseArrayLit()
	case lexer.MATCH:
		return p.parseMatch()
	case lexer.IF:
		return p.parseIfExpr()
	case lexer.ASYNC:
		p.next()
		return p.parseArrowFromParen(true)
	case lexer.FATARROW:
		p.errorf("nieoczekiwany token '=>'")
		p.next()
		return &ast.NullLit{}
	default:
		p.errorf("nieoczekiwany token %q (%s) w wyrażeniu", p.cur.Literal, p.cur.Type)
		p.next()
		return &ast.NullLit{}
	}
}

// if jako wyrażenie: let x = if cond { a } else { b }
func (p *Parser) parseIfExpr() ast.Expr {
	stmt := p.parseIf().(*ast.IfStmt)
	return &ast.CallExpr{
		Callee: &ast.ArrowFnExpr{Body: &ast.BlockStmt{Statements: []ast.Stmt{stmt}}},
	}
}

// `Ident {` jest literałem struktury, chyba że parsujemy w kontekście, gdzie
// `{` jednoznacznie rozpoczyna blok (warunek if/while, subject match,
// iterable for-in) - patrz parseExprNoStruct.
func (p *Parser) canBeStructLit() bool { return p.noStructLit == 0 }

func (p *Parser) parseStructLit(name string) ast.Expr {
	p.expect(lexer.LBRACE)
	lit := &ast.StructLit{Name: name, Values: map[string]ast.Expr{}}
	for !p.curIs(lexer.RBRACE) && !p.curIs(lexer.EOF) {
		fname := p.expect(lexer.IDENT).Literal
		var val ast.Expr
		if p.curIs(lexer.COLON) {
			p.next()
			val = p.parseExpression(ASSIGNP + 1)
		} else {
			val = &ast.Ident{Name: fname} // shorthand { x }
		}
		lit.Order = append(lit.Order, fname)
		lit.Values[fname] = val
		if p.curIs(lexer.COMMA) {
			p.next()
		}
	}
	p.expect(lexer.RBRACE)
	return lit
}

func (p *Parser) parseArrayLit() ast.Expr {
	p.expect(lexer.LBRACKET)
	var elems []ast.Expr
	for !p.curIs(lexer.RBRACKET) && !p.curIs(lexer.EOF) {
		elems = append(elems, p.parseExpression(ASSIGNP+1))
		if p.curIs(lexer.COMMA) {
			p.next()
		}
	}
	p.expect(lexer.RBRACKET)
	return &ast.ArrayLit{Elements: elems}
}

// Rozstrzyga między (expr) a (params) => body / (params) => { ... }
func (p *Parser) parseParenOrArrow() ast.Expr {
	return p.parseArrowFromParen(false)
}

func (p *Parser) parseArrowFromParen(isAsync bool) ast.Expr {
	start := p.saveState()
	if p.tryArrowParams() {
		p.restoreState(start)
		p.expect(lexer.LPAREN)
		params := p.parseParamList()
		p.expect(lexer.RPAREN)
		if p.curIs(lexer.FATARROW) {
			p.next()
			fn := &ast.ArrowFnExpr{Params: params, IsAsync: isAsync}
			if p.curIs(lexer.LBRACE) {
				fn.Body = p.parseBlock()
			} else {
				fn.ExprBody = p.parseExpression(ASSIGNP + 1)
			}
			return fn
		}
	}
	p.restoreState(start)
	p.expect(lexer.LPAREN)
	expr := p.parseExpression(LOWEST)
	p.expect(lexer.RPAREN)
	return expr
}

// tryArrowParams heurystycznie sprawdza (bez modyfikacji trwałego stanu poza
// tokenami - przywracamy stan wywołującym restoreState) czy `(...)` domyka się
// tokenem `=>`.
func (p *Parser) tryArrowParams() bool {
	depth := 0
	// symulacja: skanujemy tokeny naprzód aż zamkniemy nawias na depth 0,
	// następnie patrzymy czy kolejny token to FATARROW.
	saved := p.saveState()
	defer p.restoreState(saved)

	if !p.curIs(lexer.LPAREN) {
		return false
	}
	depth++
	p.next()
	for depth > 0 {
		if p.curIs(lexer.EOF) {
			return false
		}
		if p.curIs(lexer.LPAREN) {
			depth++
		} else if p.curIs(lexer.RPAREN) {
			depth--
		}
		p.next()
	}
	return p.curIs(lexer.FATARROW)
}

// ---- proste zapisywanie/przywracanie stanu parsera (re-lexing) ----

type parserState struct {
	lexerCopy lexer.Lexer
	cur, peek lexer.Token
}

func (p *Parser) saveState() parserState {
	return parserState{lexerCopy: *p.l, cur: p.cur, peek: p.peek}
}

func (p *Parser) restoreState(s parserState) {
	*p.l = s.lexerCopy
	p.cur = s.cur
	p.peek = s.peek
}
