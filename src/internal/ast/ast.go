package ast

// Stmt i Expr są celowo pustymi interfejsami - rozróżniane przez type switch
// w parserze/codegenie/checkerze. Upraszcza to drzewo o setki linii
// boilerplate'u, kosztem sprawdzania typów w czasie kompilacji hyperc
// (a nie w czasie pisania hyperc). To świadomy kompromis na etapie v0.1.
type Stmt interface{}
type Expr interface{}

// Pos to pozycja w źródle, używana przez checker/parser do diagnostyk.
// Nie każdy węzeł ją niesie - tylko te, dla których błędy są najbardziej
// prawdopodobne i najbardziej użyteczne do zlokalizowania. File jest
// wypełniane PO sparsowaniu przez bundler (patrz ast.StampFile) - sam
// parser go nie zna, bo nie dostaje ścieżki pliku.
type Pos struct {
	Line int
	Col  int
	File string
}

type Program struct {
	Statements []Stmt
}

// ---------- Deklaracje / instrukcje ----------

type ImportedName struct {
	Name  string
	Alias string // "" jeśli brak `as`
}

type ImportStmt struct {
	Default string         // domyślny import, "" jeśli brak
	Named   []ImportedName // importy nazwane { a, b as c }
	Path    string         // ścieżka modułu (bez cudzysłowów)
	Pos     Pos
}

type LetStmt struct {
	Name    string
	Type    string // "" => inferowany
	Mutable bool
	Const   bool
	Value   Expr
	Pos     Pos
}

// DestructureTarget to jeden element wzorca destrukturyzacji.
// Dla obiektu: { klucz: nazwa } (Key != Name gdy jest przemianowanie).
// Dla tablicy: Key jest nieużywane, liczy się tylko Name/pozycja.
type DestructureTarget struct {
	Name string
	Key  string // tylko dla obiektu; "" dla tablicy
	Rest bool   // ...reszta
}

// let { x, y: renamed } = obj   albo   let [a, b, ...reszta] = tablica
type DestructureLetStmt struct {
	IsArray bool
	Targets []DestructureTarget
	Mutable bool
	Const   bool
	Value   Expr
	Pos     Pos
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
	Pos        Pos
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
	Pos      Pos
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
	Pos      Pos
}

type ImplDecl struct {
	Target  string
	Trait   string // "" jeśli impl bezpośredni (bez `for Trait`)
	Methods []*FnDecl
	Pos     Pos
}

type TraitDecl struct {
	Name    string
	Methods []Param // tylko sygnatury, pomijane w codegenie (structural typing)
	Pos     Pos
}

type DeclareModuleStmt struct {
	Path string
	Body []Stmt
}

type BlockStmt struct {
	Statements []Stmt
}

type ExprStmt struct {
	X   Expr
	Pos Pos
}

type ReturnStmt struct {
	Value Expr // nil => `return`
	Pos   Pos
}

type IfStmt struct {
	Cond Expr
	Then *BlockStmt
	Else Stmt // *BlockStmt, *IfStmt albo nil
	Pos  Pos
}

type WhileStmt struct {
	Cond Expr
	Body *BlockStmt
	Pos  Pos
}

type ForInStmt struct {
	VarName  string
	Iterable Expr
	Body     *BlockStmt
	Pos      Pos
}

type BreakStmt struct{}
type ContinueStmt struct{}

type ThrowStmt struct {
	Value Expr
	Pos   Pos
}

type TryStmt struct {
	Try        *BlockStmt
	CatchParam string // "" jeśli `catch` bez zmiennej błędu
	Catch      *BlockStmt
	Finally    *BlockStmt
	Pos        Pos
}

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
	Pos     Pos
}

// ---------- Wyrażenia ----------

type Ident struct {
	Name string
	Pos  Pos
}
type IntLit struct{ Value string }
type FloatLit struct{ Value string }
type StringLit struct{ Value string }   // surowa treść, BEZ cudzysłowów
type TemplateLit struct{ Value string } // surowa treść, WRAZ z backtickami
type BoolLit struct{ Value bool }
type NullLit struct{}

type ArrayLit struct{ Elements []Expr }

type StructLit struct {
	Name   string
	Order  []string
	Values map[string]Expr
	Pos    Pos
}

// ObjectLit to literał obiektu BEZ nazwy typu: { klucz: wartość }.
// W przeciwieństwie do StructLit nie jest sprawdzany pod kątem znanych pól -
// to zwykły "worek" wartości, jak zwykły obiekt JS.
type ObjectLit struct {
	Order  []string
	Values map[string]Expr
	Pos    Pos
}

type BinaryExpr struct {
	Op    string
	Left  Expr
	Right Expr
	Pos   Pos
}

type UnaryExpr struct {
	Op      string
	Operand Expr
}

type AssignExpr struct {
	Target Expr
	Op     string // = += -= *= /=
	Value  Expr
	Pos    Pos
}

type CallExpr struct {
	Callee Expr
	Args   []Expr
	Pos    Pos
}

type NewExpr struct {
	Callee Expr
	Args   []Expr
}

type MemberExpr struct {
	Object   Expr
	Property string
	Optional bool // ?.
	Pos      Pos
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
type SelfExpr struct{ Pos Pos }

// ...wartość - rozwinięcie w literale tablicy, argumentach wywołania albo
// literale obiektu ({ ...inne, klucz: wartość }).
type SpreadExpr struct{ Value Expr }

type NullishExpr struct {
	Left  Expr
	Right Expr
}

// cond ? gdyPrawda : gdyFałsz
type TernaryExpr struct {
	Cond Expr
	Then Expr
	Else Expr
	Pos  Pos
}
