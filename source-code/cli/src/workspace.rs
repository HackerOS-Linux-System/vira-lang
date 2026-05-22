use std::path::{Path, PathBuf};
use anyhow::{Context, Result};
use colored::Colorize;

#[derive(Debug, Clone)]
pub struct Workspace {
    pub root: PathBuf,
    pub name: String,
    pub version: String,
    pub members: Vec<WorkspaceMember>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceMember {
    pub name: String,
    pub path: PathBuf,
    /// Relative path from workspace root
    pub rel_path: String,
}

impl Workspace {
    /// Load workspace from project.hk in the given directory
    pub fn load(dir: &Path) -> Result<Self> {
        let hk_path = dir.join("project.hk");
        let content = std::fs::read_to_string(&hk_path)
        .with_context(|| format!("Cannot find project.hk in {}", dir.display()))?;

        // Parse native HK format
        let doc = crate::hk::parse_hk(&content)
        .map_err(|e| anyhow::anyhow!("Invalid project.hk: {e}"))?;

        let name = crate::hk::get_str(&doc, "workspace", "name")
        .unwrap_or("workspace").to_owned();
        let version = crate::hk::get_str(&doc, "workspace", "version")
        .unwrap_or("0.1.0").to_owned();
        let member_paths = crate::hk::get_str_vec(&doc, "workspace", "members");

        let mut members = Vec::new();
        for rel_path in member_paths {
            let abs_path = dir.join(&rel_path);
            // Get member name from its vira.hk or last path component
            let member_name = load_member_name(&abs_path)
            .unwrap_or_else(|| {
                abs_path.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| rel_path.clone())
            });
            members.push(WorkspaceMember {
                name: member_name,
                path: abs_path,
                rel_path,
            });
        }

        Ok(Workspace { root: dir.to_path_buf(), name, version, members })
    }

    /// Build all members in the workspace
    pub fn build_all(&self, release: bool, verbose: bool) -> Result<()> {
        println!("\n{} workspace {} ({} members)\n",
                 "Building".green().bold(),
                 self.name.cyan().bold(),
                 self.members.len()
        );

        let mut failed = Vec::new();

        for member in &self.members {
            println!("  {} {} ({})",
                     "◈".cyan(),
                     member.name.bold(),
                     member.rel_path.dimmed()
            );

            if !member.path.exists() {
                eprintln!("    {} Member path does not exist: {}", "✗".red(), member.path.display());
                failed.push(member.name.clone());
                continue;
            }

            match crate::cmd_build_member(&member.path, release, verbose) {
                Ok(_)  => println!("    {} Done", "✓".green()),
                Err(e) => {
                    eprintln!("    {} {e}", "✗".red());
                    failed.push(member.name.clone());
                }
            }
            println!();
        }

        if failed.is_empty() {
            println!("{} All {} members built successfully\n",
                     "✓".green().bold(), self.members.len());
            Ok(())
        } else {
            anyhow::bail!("Failed members: {}", failed.join(", "))
        }
    }

    /// Check if a directory is a workspace root
    pub fn is_workspace(dir: &Path) -> bool {
        dir.join("project.hk").exists()
    }
}

fn load_member_name(member_dir: &Path) -> Option<String> {
    let hk_path = if member_dir.join("vira.hk").exists() {
        member_dir.join("vira.hk")
    } else {
        member_dir.join("vira.toml")
    };
    let content = std::fs::read_to_string(hk_path).ok()?;
    let val: toml::Value = content.parse().ok()?;
    val.get("package")?.get("name")?.as_str().map(|s| s.to_owned())
}

/// Generate a project.hk for a new workspace
pub fn gen_project_hk(workspace_name: &str, members: &[&str]) -> String {
    let members_arr = members.iter()
    .map(|m| format!("\"{m}\""))
    .collect::<Vec<_>>()
    .join(", ");

    // Real HK format — see: https://hackeros-linux-system.github.io/HackerOS-Website/tools-docs/hk.html
    format!(
        "! project.hk — Vira Workspace
        ! Format: HackerOS .hk

        [workspace]
        -> name    => {workspace_name}
        -> version => 0.1.0
        -> members => [{members_arr}]

        [build]
        ! Wspólny katalog budowania dla wszystkich członków
        -> output => build
        ",
        workspace_name = workspace_name,
        members_arr = members_arr,
    )
}

/// Create a new workspace scaffold
pub fn create_workspace(name: &str) -> Result<()> {
    let dir = Path::new(name);
    if dir.exists() {
        anyhow::bail!("Directory '{}' already exists", name);
    }

    std::fs::create_dir_all(dir.join("build"))?;
    // Create a default app member
    std::fs::create_dir_all(dir.join("app/src"))?;

    std::fs::write(dir.join("project.hk"), gen_project_hk(name, &["app"]))?;
    std::fs::write(dir.join("app/vira.hk"),
                   "! vira.hk — projekt Vira\n! Format: HackerOS .hk\n\n[package]\n-> name    => app\n-> version => 0.1.0\n\n[build]\n-> entry  => src/main.vira\n-> target => tauri\n"
    )?;
    std::fs::write(dir.join("app/src/main.vira"),
                   ";; app/src/main.vira\nuse <tauri>\n\npub fn main() -> void! {\n    println(\"Hello from workspace!\")\n}\n"
    )?;
    std::fs::write(dir.join(".gitignore"), "build/\ntarget/\n")?;

    println!("{} workspace {}", "Created".green().bold(), name.cyan().bold());
    println!();
    println!("  {name}/");
    println!("  ├── project.hk     ← workspace root");
    println!("  ├── app/");
    println!("  │   ├── vira.hk    ← member project");
    println!("  │   └── src/main.vira");
    println!("  └── build/         ← shared output");
    println!();
    println!("  {} cd {name}", "→".cyan());
    println!("  {} vira build-ws   (build all members)", "→".cyan());
    println!();

    Ok(())
}
