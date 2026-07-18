package ast

// Stmt i Expr są celowo pustymi interfejsami - rozróżniane przez type switch
// w parserze/codegenie. Upraszcza to drzewo o setki linii boilerplate'u,
// kosztem sprawdzania typów w czasie kompilacji hyperc (a nie w czasie
// pisania hyperc). To świadomy kompromis na etapie v0.1.
type Stmt interface{}
type Expr interface{}

type Program struct {
	Statements []Stmt
}

// ---------- Deklaracje / instrukcje ----------

type ImportStmt struct {
	Default string   // domyślny import, "" jeśli brak
	Named   []string // importy nazwane { a, b }
	Path    string   // ścieżka modułu (bez cudzysłowów)
}

type LetStmt struct {
	Name    string
	Type    string // "" => inferowany
	Mutable bool
	Const   bool
	Value   Expr
}

type TypeAliasStmt struct {
	Name   string
	Params []string // parametry generyczne aliasu, np. type Result<T,E>
	Type   string
}

type Param struct {
	Name    string
	Type    string
	Default Expr
	Rest    bool // ...args
}

type FnDecl struct {
	Name       string
	Generics   []string
	Params     []Param
	ReturnType string
	Body       *BlockStmt
	ExprBody   Expr // fn x() => expr
	IsAsync    bool
	Pub        bool
}

type StructField struct {
	Name    string
	Type    string
	Default Expr
}

type StructDecl struct {
	Name     string
	Generics []string
	Fields   []StructField
	Pub      bool
}

type EnumVariant struct {
	Name   string
	Fields []Param // pola pozycyjne wariantu
}

type EnumDecl struct {
	Name     string
	Generics []string
	Variants []EnumVariant
	Pub      bool
}

type ImplDecl struct {
	Target   string
	Trait    string // "" jeśli impl bezpośredni (bez `for Trait`)
	Methods  []*FnDecl
}

type TraitDecl struct {
	Name    string
	Methods []Param // tylko sygnatury, pomijane w codegenie (structural typing)
}

type DeclareModuleStmt struct {
	Path string
	Body []Stmt
}

type BlockStmt struct {
	Statements []Stmt
}

type ExprStmt struct{ X Expr }

type ReturnStmt struct{ Value Expr } // Value == nil => `return`

type IfStmt struct {
	Cond Expr
	Then *BlockStmt
	Else Stmt // *BlockStmt, *IfStmt albo nil
}

type WhileStmt struct {
	Cond Expr
	Body *BlockStmt
}

type ForInStmt struct {
	VarName  string
	Iterable Expr
	Body     *BlockStmt
}

type BreakStmt struct{}
type ContinueStmt struct{}

type Pattern struct {
	Wildcard bool     // `_`
	Variant  string   // nazwa wariantu enuma, albo "" dla zwykłego literału/identyfikatora
	Bindings []string // Circle(r) -> ["r"]
	Literal  Expr     // dla wzorców literałowych (np. match on 1, "a")
}

type MatchArm struct {
	Pattern *Pattern
	Body    Expr // wyrażenie, do którego ewaluuje ramię (blok traktowany jako IIFE)
}

type MatchExpr struct {
	Subject Expr
	Arms    []MatchArm
}

// ---------- Wyrażenia ----------

type Ident struct{ Name string }
type IntLit struct{ Value string }
type FloatLit struct{ Value string }
type StringLit struct{ Value string }   // surowa treść, BEZ cudzysłowów
type TemplateLit struct{ Value string } // surowa treść, WRAZ z backtickami
type BoolLit struct{ Value bool }
type NullLit struct{}

type ArrayLit struct{ Elements []Expr }

type StructLit struct {
	Name  string
	Order []string
	Values map[string]Expr
}

type BinaryExpr struct {
	Op    string
	Left  Expr
	Right Expr
}

type UnaryExpr struct {
	Op      string
	Operand Expr
}

type AssignExpr struct {
	Target Expr
	Op     string // = += -= *= /=
	Value  Expr
}

type CallExpr struct {
	Callee Expr
	Args   []Expr
}

type NewExpr struct {
	Callee Expr
	Args   []Expr
}

type MemberExpr struct {
	Object   Expr
	Property string
	Optional bool // ?.
}

type IndexExpr struct {
	Object Expr
	Index  Expr
}

type ArrowFnExpr struct {
	Params   []Param
	Body     *BlockStmt
	ExprBody Expr
	IsAsync  bool
}

type AwaitExpr struct{ Value Expr }
type SelfExpr struct{}

type NullishExpr struct {
	Left  Expr
	Right Expr
}

type MatchExprWrapper struct{ Match *MatchExpr } // match używany jako wyrażenie w pozycji Expr
