use vira_parser::ast::*;
use crate::native_api::{NativeApi, NativeApiKind, NativeApiRegistry};

pub struct CodegenContext {
    pub output: String,
    pub indent: usize,
    pub native_apis: Vec<NativeApi>,
    api_registry: NativeApiRegistry,
    in_impl_block: bool,
    in_main_fn: bool,
}

impl CodegenContext {
    pub fn new() -> Self {
        CodegenContext {
            output: String::new(),
            indent: 0,
            native_apis: Vec::new(),
            api_registry: NativeApiRegistry::new(),
            in_impl_block: false,
            in_main_fn: false,
        }
    }

    // ── Emit helpers ──────────────────────────────────────────────────────────

    #[allow(dead_code)]
    fn emit(&mut self, s: &str) {
        self.output.push_str(s);
    }

    fn emitln(&mut self, s: &str) {
        let indent = "    ".repeat(self.indent);
        self.output.push_str(&indent);
        self.output.push_str(s);
        self.output.push('\n');
    }

    fn emitln_raw(&mut self, s: &str) {
        self.output.push_str(s);
        self.output.push('\n');
    }

    fn blank(&mut self) {
        self.output.push('\n');
    }

    fn indent(&mut self) { self.indent += 1; }
    fn dedent(&mut self) { if self.indent > 0 { self.indent -= 1; } }

    fn indent_str(&self) -> String {
        "    ".repeat(self.indent)
    }

    // ── Entry point ───────────────────────────────────────────────────────────

    pub fn generate(&mut self, program: &Program) -> String {
        // File header
        // Resolve and emit imports
        self.generate_imports(&program.imports);
        self.blank();

        // Emit items
        for item in &program.items {
            self.generate_item(item);
            self.blank();
        }

        self.output.clone()
    }

    // ── Imports ───────────────────────────────────────────────────────────────

    fn generate_imports(&mut self, imports: &[Import]) {
        for import in imports {
            match &import.kind {
                ImportKind::Native => {
                    // use <tauri> / use <gtk:4> / use <qt:5> / use <android> / use <slint>
                    if let Some(api) = self.api_registry.resolve(
                        &import.name,
                        import.version.as_deref(),
                    ) {
                        // Warn for placeholder targets
                        if api.kind == NativeApiKind::Android {
                            self.emitln_raw(
                                "// [VIRA] Android target — Kotlin transpiler is a future feature",
                            );
                        }
                        for prelude in &api.rust_prelude {
                            self.emitln_raw(prelude);
                        }
                        self.native_apis.push(api);
                    } else {
                        self.emitln_raw(&format!(
                            "// WARNING: unknown native API '{}' — skipping",
                            import.name
                        ));
                    }
                }
                ImportKind::Ecosystem { ecosystem } => {
                    // using <serde> from <crates>
                    // using <react> from <npm>
                    let api = self.api_registry.resolve_ecosystem(
                        &import.name,
                        import.version.as_deref(),
                                                                  ecosystem,
                    );
                    for prelude in &api.rust_prelude {
                        self.emitln_raw(prelude);
                    }
                    self.native_apis.push(api);
                }
                ImportKind::ViraRegistry => {
                    // usage <name> — vira.io placeholder
                    self.emitln_raw(&format!(
                        "// [VIRA] vira.io: {} — registry not yet available",
                        import.name
                    ));
                }
            }
        }
    }

    // ── Items ─────────────────────────────────────────────────────────────────

    fn generate_item(&mut self, item: &Item) {
        match item {
            Item::Function(f) => self.generate_fn(f),
            Item::Struct(s)   => self.generate_struct(s),
            Item::Enum(e)     => self.generate_enum(e),
            Item::Trait(t)    => self.generate_trait(t),
            Item::Impl(i)     => self.generate_impl(i),
            Item::TypeAlias(a) => self.generate_type_alias(a),
            Item::Constant(c) => self.generate_const(c),
            Item::ExternBlock(e) => self.generate_extern(e),
        }
    }

    // ── Functions ─────────────────────────────────────────────────────────────

    fn generate_fn(&mut self, f: &FunctionDef) {
        // Doc comments
        for doc in &f.docs {
            self.emitln(&format!("/// {doc}"));
        }

        // Detect tauri command: pub fn in tauri context
        let old_in_main = self.in_main_fn;
        self.in_main_fn = f.name == "main";
        let is_tauri_command = f.visibility == Visibility::Public
        && self.has_tauri_api();
        if is_tauri_command {
            self.emitln("#[tauri::command]");
        }

        // Signature
        let mut sig = String::new();
        sig.push_str(&self.vis_str(&f.visibility));
        if f.is_async { sig.push_str("async "); }
        if f.is_unsafe { sig.push_str("unsafe "); }
        if f.is_inline { sig.push_str("#[inline] "); }
        sig.push_str("fn ");
        sig.push_str(&f.name);

        if !f.generics.is_empty() {
            sig.push('<');
            sig.push_str(&self.generics_str(&f.generics));
            sig.push('>');
        }

        sig.push('(');
        let params: Vec<String> = f.params.iter().map(|p| self.param_str(p)).collect();
        sig.push_str(&params.join(", "));
        sig.push(')');

        if let Some(ret) = &f.return_type {
            sig.push_str(" -> ");
            sig.push_str(&self.type_str(ret));
        }

        if !f.where_clause.is_empty() {
            sig.push_str("\nwhere\n");
            for pred in &f.where_clause {
                sig.push_str(&format!(
                    "    {}: {},\n",
                    self.type_str(&pred.ty),
                                      pred.bounds.iter().map(|b| self.type_str(b)).collect::<Vec<_>>().join(" + ")
                ));
            }
        }

        if let Some(body) = &f.body {
            let ind = self.indent_str();
            self.emitln_raw(&format!("{ind}{sig} {{"));
            self.indent();
            self.generate_block_body(body);
            self.dedent();
            self.emitln("}");
        } else {
            // Abstract / extern signature
            self.emitln(&format!("{sig};"));
        }
        self.in_main_fn = old_in_main;
    }

    fn param_str(&self, p: &Param) -> String {
        if p.is_self {
            if self.in_impl_block { return "&mut self".to_owned(); }
            return "self".to_owned();
        }
        format!("{}: {}", p.name, self.type_str(&p.ty))
    }

    // ── Structs ───────────────────────────────────────────────────────────────

    fn generate_struct(&mut self, s: &StructDef) {
        for doc in &s.docs {
            self.emitln(&format!("/// {doc}"));
        }

        // Always derive common traits
        self.emitln("#[derive(Debug, Clone)]");

        // If tauri context, also derive Serialize/Deserialize
        if self.has_tauri_api() {
            self.emitln("#[derive(serde::Serialize, serde::Deserialize)]");
        }

        let mut header = format!("{}struct {}", self.vis_str(&s.visibility), s.name);
        if !s.generics.is_empty() {
            header.push('<');
            header.push_str(&self.generics_str(&s.generics));
            header.push('>');
        }
        self.emitln(&format!("{header} {{"));
        self.indent();

        for field in &s.fields {
            for doc in &field.docs {
                self.emitln(&format!("/// {doc}"));
            }
            self.emitln(&format!(
                "{}{}: {},",
                self.vis_str(&field.visibility),
                                 field.name,
                                 self.type_str(&field.ty)
            ));
        }

        self.dedent();
        self.emitln("}");
    }

    // ── Enums ─────────────────────────────────────────────────────────────────

    fn generate_enum(&mut self, e: &EnumDef) {
        for doc in &e.docs {
            self.emitln(&format!("/// {doc}"));
        }
        self.emitln("#[derive(Debug, Clone, PartialEq)]");

        let mut header = format!("{}enum {}", self.vis_str(&e.visibility), e.name);
        if !e.generics.is_empty() {
            header.push('<');
            header.push_str(&self.generics_str(&e.generics));
            header.push('>');
        }

        self.emitln(&format!("{header} {{"));
        self.indent();

        for variant in &e.variants {
            for doc in &variant.docs {
                self.emitln(&format!("/// {doc}"));
            }
            match &variant.fields {
                EnumVariantFields::Unit => {
                    self.emitln(&format!("{},", variant.name));
                }
                EnumVariantFields::Tuple(types) => {
                    let ts: Vec<_> = types.iter().map(|t| self.type_str(t)).collect();
                    self.emitln(&format!("{}({}),", variant.name, ts.join(", ")));
                }
                EnumVariantFields::Struct(fields) => {
                    self.emitln(&format!("{} {{", variant.name));
                    self.indent();
                    for f in fields {
                        self.emitln(&format!("{}: {},", f.name, self.type_str(&f.ty)));
                    }
                    self.dedent();
                    self.emitln("},");
                }
            }
        }

        self.dedent();
        self.emitln("}");
    }

    // ── Traits ────────────────────────────────────────────────────────────────

    fn generate_trait(&mut self, t: &TraitDef) {
        for doc in &t.docs {
            self.emitln(&format!("/// {doc}"));
        }

        let mut header = format!("{}trait {}", self.vis_str(&t.visibility), t.name);
        if !t.generics.is_empty() {
            header.push('<');
            header.push_str(&self.generics_str(&t.generics));
            header.push('>');
        }
        if !t.supertraits.is_empty() {
            header.push_str(": ");
            header.push_str(
                &t.supertraits
                .iter()
                .map(|s| self.type_str(s))
                .collect::<Vec<_>>()
                .join(" + "),
            );
        }

        self.emitln(&format!("{header} {{"));
        self.indent();

        for item in &t.items {
            match item {
                TraitItem::Method(f) => self.generate_fn(f),
                TraitItem::AssocType(name, default) => {
                    if let Some(def) = default {
                        self.emitln(&format!("type {} = {};", name, self.type_str(def)));
                    } else {
                        self.emitln(&format!("type {};", name));
                    }
                }
                TraitItem::Constant(c) => self.generate_const(c),
            }
        }

        self.dedent();
        self.emitln("}");
    }

    // ── Impl ──────────────────────────────────────────────────────────────────

    fn generate_impl(&mut self, i: &ImplBlock) {
        let mut header = String::from("impl");
        if !i.generics.is_empty() {
            header.push('<');
            header.push_str(&self.generics_str(&i.generics));
            header.push('>');
        }
        header.push(' ');
        if let Some(tr) = &i.trait_name {
            header.push_str(&self.type_str(tr));
            header.push_str(" for ");
        }
        header.push_str(&self.type_str(&i.self_type));

        if !i.where_clause.is_empty() {
            header.push_str("\nwhere\n");
            for pred in &i.where_clause {
                header.push_str(&format!(
                    "    {}: {},\n",
                    self.type_str(&pred.ty),
                                         pred.bounds.iter().map(|b| self.type_str(b)).collect::<Vec<_>>().join(" + ")
                ));
            }
        }

        self.emitln(&format!("{header} {{"));
        self.indent();
        self.in_impl_block = true;
        for item in &i.items {
            self.generate_item(item);
        }
        self.in_impl_block = false;
        self.dedent();
        self.emitln("}");
    }

    // ── Type alias ────────────────────────────────────────────────────────────

    fn generate_type_alias(&mut self, a: &TypeAlias) {
        for doc in &a.docs {
            self.emitln(&format!("/// {doc}"));
        }
        let mut line = format!("{}type {}", self.vis_str(&a.visibility), a.name);
        if !a.generics.is_empty() {
            line.push('<');
            line.push_str(&self.generics_str(&a.generics));
            line.push('>');
        }
        line.push_str(&format!(" = {};", self.type_str(&a.ty)));
        self.emitln(&line);
    }

    // ── Constants ─────────────────────────────────────────────────────────────

    fn generate_const(&mut self, c: &ConstDef) {
        for doc in &c.docs {
            self.emitln(&format!("/// {doc}"));
        }
        self.emitln(&format!(
            "{}const {}: {} = {};",
            self.vis_str(&c.visibility),
                             c.name,
                             self.type_str(&c.ty),
                             self.expr_str(&c.value)
        ));
    }

    // ── Extern ────────────────────────────────────────────────────────────────

    fn generate_extern(&mut self, e: &ExternBlock) {
        let abi = e.abi.as_deref().unwrap_or("C");
        self.emitln(&format!("extern \"{abi}\" {{"));
        self.indent();
        for f in &e.items {
            self.generate_fn(f);
        }
        self.dedent();
        self.emitln("}");
    }

    // ── Blocks ────────────────────────────────────────────────────────────────

    fn generate_block_body(&mut self, block: &Block) {
        for stmt in &block.stmts {
            self.generate_stmt(stmt);
        }
        if let Some(tail) = &block.tail {
            self.emitln(&self.expr_str(tail));
        }
    }

    fn block_str(&self, block: &Block) -> String {
        let mut inner = String::from("{\n");
        for stmt in &block.stmts {
            inner.push_str(&self.stmt_str(stmt));
            inner.push('\n');
        }
        if let Some(tail) = &block.tail {
            inner.push_str(&self.expr_str(tail));
            inner.push('\n');
        }
        inner.push('}');
        inner
    }

    // ── Statements ────────────────────────────────────────────────────────────

    fn generate_stmt(&mut self, stmt: &Stmt) {
        let line = self.stmt_str(stmt);
        self.emitln(&line);
    }

    fn stmt_str(&self, stmt: &Stmt) -> String {
        match stmt {
            Stmt::Let(l) => {
                let ty = l.ty.as_ref().map(|t| format!(": {}", self.type_str(t))).unwrap_or_default();
                let val = l.value.as_ref().map(|v| format!(" = {}", self.expr_str(v))).unwrap_or_default();
                format!("let {}{}{};", self.pattern_str(&l.name), ty, val)
            }
            Stmt::Var(v) => {
                // var → let mut in Rust
                let ty = v.ty.as_ref().map(|t| format!(": {}", self.type_str(t))).unwrap_or_default();
                let val = v.value.as_ref().map(|e| format!(" = {}", self.expr_str(e))).unwrap_or_default();
                format!("let mut {}{}{};", self.pattern_str(&v.name), ty, val)
            }
            Stmt::Return(val, _) => {
                match val {
                    Some(e) => format!("return {};", self.expr_str(e)),
                    None => "return;".to_owned(),
                }
            }
            Stmt::Break(val, _) => {
                match val {
                    Some(e) => format!("break {};", self.expr_str(e)),
                    None => "break;".to_owned(),
                }
            }
            Stmt::Continue(_) => "continue;".to_owned(),
            Stmt::Defer(e, _) => {
                // Defer → scopeguard or manual Drop in Rust
                format!("let _defer_guard = ::scopeguard::defer(|| {{ {}; }});", self.expr_str(e))
            }
            Stmt::Throw(e, _) => {
                let expr_s = self.expr_str(e);
                crate::error_handling::gen_throw(&expr_s)
            }
            Stmt::Expr(e) => {
                format!("{};", self.expr_str(e))
            }
            Stmt::Item(item) => {
                // Nested items inline — emit as string
                let mut ctx = CodegenContext::new();
                ctx.native_apis = self.native_apis.clone();
                ctx.generate_item(item);
                ctx.output
            }
        }
    }

    // ── Expressions ───────────────────────────────────────────────────────────

    fn expr_str(&self, expr: &Expr) -> String {
        match &expr.kind {
            ExprKind::Literal(lit) => self.literal_str(lit),
            ExprKind::Ident(name) => name.clone(),
            ExprKind::Path(segments) => {
                let mapped: Vec<String> = segments.iter().map(|s| Self::map_mod(s)).collect();
                // gtk::ApplicationFlags → gio::ApplicationFlags (it's in gio crate)
                let joined = mapped.join("::");
                if joined == "gtk4::ApplicationFlags" {
                    "gio::ApplicationFlags".to_owned()
                } else {
                    joined
                }
            }
            ExprKind::MacroCall(path, bracket, args) => {
                let path_str = path.iter().map(|s| Self::map_mod(s)).collect::<Vec<_>>().join("::");
                let (open, close) = match bracket {
                    '[' => ('[', ']'),
                    '{' => ('{', '}'),
                    _   => ('(', ')'),
                };
                let args_str: Vec<String> = args.iter().map(|a| self.expr_str(a)).collect();
                format!("{}!{}{}{}", path_str, open, args_str.join(", "), close)
            }
            ExprKind::SelfExpr => "self".to_owned(),

            ExprKind::Binary(op, lhs, rhs) => {
                let ls = self.expr_str(lhs);
                let rs = self.expr_str(rhs);
                match op {
                    // Use format!() for Add to avoid Rust String+String type errors
                    BinOp::Add => {
                        // Build format!() call for string concatenation
                        // avoids Rust String+String type errors
                        let fmtcall = String::from("format!(\"{}{}\"");
                        format!("{fmtcall}, {ls}, {rs})")
                    }
                    _ => format!("({ls} {} {rs})", self.binop_str(op)),
                }
            }
            ExprKind::Unary(op, e) => {
                match op {
                    UnaryOp::Neg   => format!("(-{})", self.expr_str(e)),
                    UnaryOp::Not   => format!("(!{})", self.expr_str(e)),
                    UnaryOp::Deref => format!("(*{})", self.expr_str(e)),
                    UnaryOp::Ref   => format!("(&{})", self.expr_str(e)),
                }
            }
            ExprKind::Assign(lhs, rhs) => {
                format!("{} = {}", self.expr_str(lhs), self.expr_str(rhs))
            }
            ExprKind::CompoundAssign(op, lhs, rhs) => {
                format!("{} {}= {}", self.expr_str(lhs), self.binop_str(op), self.expr_str(rhs))
            }
            ExprKind::Call(callee, args) => {
                let arg_strs: Vec<_> = args.iter().map(|a| self.expr_str(&a.value)).collect();
                format!("{}({})", self.expr_str(callee), arg_strs.join(", "))
            }
            ExprKind::MethodCall(receiver, method, _generics, args) => {
                let recv_str = self.expr_str(receiver);
                let arg_strs: Vec<_> = args.iter().map(|a| self.expr_str(&a.value)).collect();
                let args_str = arg_strs.join(", ");

                // Methods needing &str (not String) as argument
                let str_ref_methods = ["contains","starts_with","ends_with","join",
                "set_text","append","from_icon_name","push_str",
                "set_label","set_markup","set_tooltip_text"];
                // GTK4 methods needing Option<&str>
                let opt_str_methods = ["set_title","set_subtitle"];
                // GTK4 methods needing Option<&impl IsA<Widget>>
                let opt_widget_methods = ["set_child","set_start_child","set_end_child",
                "set_title_widget","set_center_widget"];

                if opt_widget_methods.contains(&method.as_str()) {
                    // Wrap with Some(&...) for GTK4 Option<&impl IsA<Widget>>
                    let wrapped = arg_strs.iter()
                    .map(|a| format!("Some(&{a})"))
                    .collect::<Vec<_>>().join(", ");
                    format!("{recv_str}.{method}({wrapped})")
                } else if opt_str_methods.contains(&method.as_str()) {
                    // Wrap with Some(...) for GTK4 Option<&str>
                    let wrapped = arg_strs.iter()
                    .map(|a| if a.starts_with('"') { format!("Some({a})") } else { format!("Some(&{a})") })
                    .collect::<Vec<_>>().join(", ");
                    format!("{recv_str}.{method}({wrapped})")
                } else if str_ref_methods.contains(&method.as_str()) {
                    let ref_args = arg_strs.iter()
                    .map(|a| if a.starts_with('"') || a.starts_with('&') { a.clone() } else { format!("&{a}") })
                    .collect::<Vec<_>>().join(", ");
                    format!("{recv_str}.{method}({ref_args})")
                } else {
                    crate::stdlib::emit_method_call(&recv_str, method, &args_str)
                }
            }
            ExprKind::Field(obj, field) => {
                format!("{}.{}", self.expr_str(obj), field)
            }
            ExprKind::Index(obj, idx) => {
                format!("{}[{}]", self.expr_str(obj), self.expr_str(idx))
            }
            ExprKind::Closure(params, ret, body) => {
                let ps: Vec<_> = params.iter().map(|p| {
                    if p.ty.is_infer() {
                        p.name.clone()
                    } else {
                        format!("{}: {}", p.name, self.type_str(&p.ty))
                    }
                }).collect();
                let ret_str = ret.as_ref().map(|r| format!(" -> {}", self.type_str(r))).unwrap_or_default();
                format!("|{}|{} {}", ps.join(", "), ret_str, self.expr_str(body))
            }
            ExprKind::Block(block) => self.block_str(block),
            ExprKind::If(cond, then, elifs, else_) => {
                let mut s = format!("if {} {}", self.expr_str(cond), self.block_str(then));
                for (ec, eb) in elifs {
                    s.push_str(&format!(" else if {} {}", self.expr_str(ec), self.block_str(eb)));
                }
                if let Some(e) = else_ {
                    s.push_str(&format!(" else {}", self.block_str(e)));
                }
                s
            }
            ExprKind::While(cond, body) => {
                format!("while {} {}", self.expr_str(cond), self.block_str(body))
            }
            ExprKind::For(pat, iter, body) => {
                format!("for {} in {} {}", self.pattern_str(pat), self.expr_str(iter), self.block_str(body))
            }
            ExprKind::Match(subject, arms) => {
                let subj_s = self.expr_str(subject);
                // Detect Option<T> match: any arm with nil/None pattern
                let is_opt = arms.iter().any(|a| matches!(
                    &a.pattern, Pattern::Literal(LiteralKind::Nil)
                ));
                let mut s = format!("match {subj_s} {{\n");
                for arm in arms {
                    let guard = arm.guard.as_ref()
                    .map(|g| format!(" if {}", self.expr_str(g)))
                    .unwrap_or_default();
                    let body = self.expr_str(&arm.body);
                    // For Option match: nil → None, ident → Some(ident)
                    let pat = match &arm.pattern {
                        Pattern::Literal(LiteralKind::Nil) => "None".to_owned(),
                        Pattern::Ident(n) if is_opt && n != "_" => format!("Some({n})"),
                        _ => self.pattern_str(&arm.pattern),
                    };
                    s.push_str(&format!("    {pat}{guard} => {body},\n"));
                }
                s.push('}');
                s
            }
            ExprKind::StructLit(name, fields) => {
                let fs: Vec<_> = fields.iter().map(|(k, v)| {
                    format!("{k}: {}", self.expr_str(v))
                }).collect();
                format!("{} {{ {} }}", name, fs.join(", "))
            }
            ExprKind::Tuple(elems) => {
                let es: Vec<_> = elems.iter().map(|e| self.expr_str(e)).collect();
                format!("({})", es.join(", "))
            }
            ExprKind::Array(elems) => {
                let es: Vec<_> = elems.iter().map(|e| self.expr_str(e)).collect();
                format!("[{}]", es.join(", "))
            }
            ExprKind::Range(lo, hi, inclusive) => {
                let lo_s = lo.as_ref().map(|e| self.expr_str(e)).unwrap_or_default();
                let hi_s = hi.as_ref().map(|e| self.expr_str(e)).unwrap_or_default();
                if *inclusive {
                    format!("{}..={}", lo_s, hi_s)
                } else {
                    format!("{}..{}", lo_s, hi_s)
                }
            }
            ExprKind::Cast(e, ty) => {
                format!("({} as {})", self.expr_str(e), self.type_str(ty))
            }
            ExprKind::Is(e, ty) => {
                // `is` → matches! in Rust
                format!("matches!({}, {})", self.expr_str(e), self.type_str(ty))
            }
            ExprKind::Try(e) => {
                let inner = self.expr_str(e);
                if self.in_main_fn && self.has_tauri_api() {
                    {
                        let msg = "Vira runtime error";
                        format!("{inner}.unwrap_or_else(|_e| panic!(\"{msg}: {{}}\", _e))")
                    }
                } else { format!("{inner}?") }
            }
            ExprKind::Await(e) => format!("{}.await", self.expr_str(e)),
            ExprKind::Spawn(e) => {
                format!("tokio::spawn(async move {{ {} }})", self.expr_str(e))
            }
            ExprKind::Comptime(e) => {
                // comptime → const evaluation hint
                format!("/* comptime */ {}", self.expr_str(e))
            }
            ExprKind::ArenaAlloc(e) => {
                // arena allocate → arena.alloc(expr)
                format!("__vira_arena.alloc({})", self.expr_str(e))
            }
            ExprKind::Ref(e) => format!("&{}", self.expr_str(e)),
            ExprKind::RefMut(e) => format!("&mut {}", self.expr_str(e)),
            ExprKind::Deref(e) => format!("*{}", self.expr_str(e)),
            ExprKind::Unsafe(block) => {
                format!("unsafe {}", self.block_str(block))
            }
        }
    }

    fn literal_str(&self, lit: &LiteralKind) -> String {
        match lit {
            LiteralKind::Int(n) => n.to_string(),
            LiteralKind::Float(f) => {
                let s = format!("{f}");
                if s.contains('.') { s } else { format!("{s}.0") }
            }
            LiteralKind::Str(s) => format!("{:?}.to_owned()", s),
            LiteralKind::Char(c) => format!("'{c}'"),
            LiteralKind::Bool(b) => b.to_string(),
            LiteralKind::Nil => "None".to_owned(),
        }
    }

    fn binop_str(&self, op: &BinOp) -> &'static str {
        match op {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::Eq => "==",
            BinOp::NotEq => "!=",
            BinOp::Lt => "<",
            BinOp::Gt => ">",
            BinOp::LtEq => "<=",
            BinOp::GtEq => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
        }
    }

    // ── Patterns ──────────────────────────────────────────────────────────────

    fn pattern_str(&self, pat: &Pattern) -> String {
        match pat {
            Pattern::Ident(name) => name.clone(),
            Pattern::Wildcard => "_".to_owned(),
            Pattern::Tuple(pats) => {
                let ps: Vec<_> = pats.iter().map(|p| self.pattern_str(p)).collect();
                format!("({})", ps.join(", "))
            }
            Pattern::Struct(name, fields) => {
                let fs: Vec<_> = fields.iter().map(|(k, v)| {
                    format!("{k}: {}", self.pattern_str(v))
                }).collect();
                format!("{} {{ {} }}", name, fs.join(", "))
            }
            Pattern::Enum(name, fields) => {
                let fs: Vec<_> = fields.iter().map(|f| self.pattern_str(f)).collect();
                if fs.is_empty() {
                    name.clone()
                } else {
                    format!("{}({})", name, fs.join(", "))
                }
            }
            Pattern::Literal(lit) => {
                // In pattern position, use &str not String::from()
                match lit {
                    LiteralKind::Str(s) => format!("{:?}", s),      // "foo"
                    LiteralKind::Int(n) => n.to_string(),
                    LiteralKind::Float(f) => {
                        let s = format!("{f}");
                        if s.contains('.') { s } else { format!("{s}.0") }
                    }
                    LiteralKind::Char(c) => format!("'{c}'"),
                    LiteralKind::Bool(b) => b.to_string(),
                    LiteralKind::Nil => "None".to_owned(),
                }
            }
            Pattern::Or(pats) => {
                pats.iter().map(|p| self.pattern_str(p)).collect::<Vec<_>>().join(" | ")
            }
            Pattern::Ref(p) => format!("ref {}", self.pattern_str(p)),
            Pattern::Range(lo, hi) => {
                format!("{}..={}", self.pattern_str(lo), self.pattern_str(hi))
            }
        }
    }

    // ── Types ─────────────────────────────────────────────────────────────────

    fn type_str(&self, ty: &TypeExpr) -> String {
        match ty {
            TypeExpr::Named(name, args) => {
                let rname = Self::map_mod(name);
                if args.is_empty() {
                    rname
                } else {
                    let a: Vec<_> = args.iter().map(|t| self.type_str(t)).collect();
                    format!("{}<{}>", rname, a.join(", "))
                }
            }
            TypeExpr::Ref(inner) => format!("&{}", self.type_str(inner)),
            TypeExpr::RefMut(inner) => format!("&mut {}", self.type_str(inner)),
            TypeExpr::Ptr(inner) => format!("*const {}", self.type_str(inner)),
            TypeExpr::Slice(inner) => format!("Vec<{}>", self.type_str(inner)),
            TypeExpr::Array(inner, len) => {
                format!("[{}; {}]", self.type_str(inner), self.expr_str(len))
            }
            TypeExpr::Tuple(types) => {
                let ts: Vec<_> = types.iter().map(|t| self.type_str(t)).collect();
                format!("({})", ts.join(", "))
            }
            TypeExpr::Function(args, ret) => {
                let as_: Vec<_> = args.iter().map(|t| self.type_str(t)).collect();
                format!("fn({}) -> {}", as_.join(", "), self.type_str(ret))
            }
            TypeExpr::Optional(inner) => format!("Option<{}>", self.type_str(inner)),
            TypeExpr::Result(ok, err) => {
                let ok_str = self.type_str(ok);
                let err_str = err.as_ref().map(|e| self.type_str(e));
                crate::error_handling::transform_result_type(&ok_str, err_str.as_deref())
            }
            TypeExpr::Never => "!".to_owned(),
            TypeExpr::Infer => "_".to_owned(),
            TypeExpr::SelfTy => "Self".to_owned(),
            TypeExpr::Void => "()".to_owned(),
        }
    }

    // ── Generics ──────────────────────────────────────────────────────────────

    fn generics_str(&self, generics: &[GenericParam]) -> String {
        generics.iter().map(|g| {
            if g.bounds.is_empty() {
                g.name.clone()
            } else {
                format!(
                    "{}: {}",
                    g.name,
                    g.bounds.iter().map(|b| self.type_str(b)).collect::<Vec<_>>().join(" + ")
                )
            }
        }).collect::<Vec<_>>().join(", ")
    }

    // ── Visibility ────────────────────────────────────────────────────────────

    fn vis_str(&self, vis: &Visibility) -> &'static str {
        match vis {
            Visibility::Public => "pub ",
            Visibility::Private => "",
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn map_mod(name: &str) -> String {
        match name {
            "gtk" => "gtk4".into(),
            other => other.into(),
        }
    }

    fn has_tauri_api(&self) -> bool {
        self.native_apis.iter().any(|a| a.kind == NativeApiKind::Tauri)
    }
}

impl Default for CodegenContext {
    fn default() -> Self {
        CodegenContext::new()
    }
}

// TypeExpr::Infer comparison uses is_infer() helper defined in ast (via PartialEq derive there)
