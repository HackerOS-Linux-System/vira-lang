
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};

// ─── Module graph ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ViraModule {
    pub name: String,
    /// Canonical path (relative to project root)
    pub path: PathBuf,
    pub source: String,
    pub kind: ModuleKind,
    /// Modules this one depends on
    pub imports: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModuleKind {
    /// src/*.vira — backend logic
    Backend,
    /// lib/*.vira — UI layer
    Ui,
    /// Generated entry point
    Main,
}

/// Full module graph for a project
pub struct ModuleGraph {
    pub modules: HashMap<String, ViraModule>,
    /// Topological order for compilation
    pub order: Vec<String>,
    pub entry: String,
}

impl ModuleGraph {
    pub fn new() -> Self {
        ModuleGraph {
            modules: HashMap::new(),
            order: Vec::new(),
            entry: String::new(),
        }
    }

    /// Load all modules from project dir
    pub fn load(project_dir: &Path, entry: &Path) -> Result<Self> {
        let mut graph = ModuleGraph::new();

        // Load entry module
        let entry_source = std::fs::read_to_string(entry)
        .with_context(|| format!("reading entry {}", entry.display()))?;

        let entry_name = entry.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "main".to_string());

        graph.entry = entry_name.clone();
        graph.modules.insert(entry_name.clone(), ViraModule {
            name: entry_name.clone(),
                             path: entry.to_path_buf(),
                             source: entry_source,
                             kind: ModuleKind::Main,
                             imports: Vec::new(),
        });

        // Scan src/ for additional modules
        let src_dir = project_dir.join("src");
        if src_dir.exists() {
            graph.scan_dir(&src_dir, project_dir, ModuleKind::Backend)?;
        }

        // Scan lib/ for UI modules
        let lib_dir = project_dir.join("lib");
        if lib_dir.exists() {
            // Only load .vira files from lib/ (not .html)
            graph.scan_dir(&lib_dir, project_dir, ModuleKind::Ui)?;
        }

        graph.compute_order();
        Ok(graph)
    }

    fn scan_dir(&mut self, dir: &Path, root: &Path, kind: ModuleKind) -> Result<()> {
        for entry in walkdir::WalkDir::new(dir).follow_links(true).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().map_or(false, |e| e == "vira") && path != root.join("src/main.vira") {
                let rel = path.strip_prefix(root).unwrap_or(path);
                let mod_name = rel.with_extension("")
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "::");
                let source = std::fs::read_to_string(path)?;
                self.modules.insert(mod_name.clone(), ViraModule {
                    name: mod_name,
                    path: path.to_path_buf(),
                                    source,
                                    kind: kind.clone(),
                                    imports: Vec::new(),
                });
            }
        }
        Ok(())
    }

    fn compute_order(&mut self) {
        // Simple: entry first, rest after (full topo sort is future work)
        let mut order = vec![self.entry.clone()];
        for name in self.modules.keys() {
            if *name != self.entry {
                order.push(name.clone());
            }
        }
        self.order = order;
    }

    /// Concatenate all sources in order (simple mode — used until full module codegen)
    pub fn concat_sources(&self) -> String {
        let mut out = String::new();
        for name in &self.order {
            if let Some(m) = self.modules.get(name) {
                if !out.is_empty() { out.push('\n'); }
                out.push_str(&m.source);
            }
        }
        out
    }

    /// Generate Rust module declarations for the entry file
    pub fn rust_mod_declarations(&self) -> String {
        let mut out = String::new();
        for name in &self.order {
            if name == &self.entry { continue; }
            if let Some(m) = self.modules.get(name) {
                let rust_mod = m.name.replace("::", "_").replace("/", "_");
                match m.kind {
                    ModuleKind::Backend => out.push_str(&format!("pub mod {};\n", rust_mod)),
                    ModuleKind::Ui      => out.push_str(&format!("// UI module: {}\n", m.name)),
                    ModuleKind::Main    => {}
                }
            }
        }
        out
    }
}

// ─── Import statement resolver ────────────────────────────────────────────────

/// Resolve `import <name>` to a file path
pub fn resolve_import(name: &str, project_dir: &Path) -> Option<PathBuf> {
    // Standard search order:
    //   1. src/<name>.vira
    //   2. lib/<name>.vira
    //   3. src/<name>/mod.vira
    let candidates = [
        project_dir.join(format!("src/{}.vira", name)),
        project_dir.join(format!("lib/{}.vira", name)),
        project_dir.join(format!("src/{}/mod.vira", name)),
        project_dir.join(format!("{}.vira", name)),
    ];
    for c in &candidates {
        if c.exists() { return Some(c.clone()); }
    }
    None
}
