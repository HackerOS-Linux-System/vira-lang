use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use std::path::{Path, PathBuf};
use std::process::Command;

mod project;
mod fmt;
mod repl;

// ─── CLI structure ────────────────────────────────────────────────────────────

/// Vira — a modern language transpiled to Rust.
/// Primary target: Tauri / GTK / Qt — the HackerOS ecosystem.
#[derive(Parser)]
#[command(
    name = "vira",
    author = "HackerOS Team",
    version = env!("CARGO_PKG_VERSION"),
    about = "Vira language toolchain",
    long_about = r#"
┌─────────────────────────────────────────┐
│  ██╗   ██╗██╗██████╗  █████╗           │
│  ██║   ██║██║██╔══██╗██╔══██╗          │
│  ██║   ██║██║██████╔╝███████║          │
│  ╚██╗ ██╔╝██║██╔══██╗██╔══██║          │
│   ╚████╔╝ ██║██║  ██║██║  ██║          │
│    ╚═══╝  ╚═╝╚═╝  ╚═╝╚═╝  ╚═╝          │
│                                         │
│  Transpiled to Rust. HackerOS native.   │
│  Tauri • GTK • Qt                       │
└─────────────────────────────────────────┘
"#
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new Vira project
    New {
        /// Project name
        name: String,
        /// Project template
        #[arg(long, default_value = "tauri")]
        template: Template,
    },

    /// Build a Vira project (transpile → Rust → cargo build)
    Build {
        /// Project directory (default: current dir)
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Build in release mode
        #[arg(long, short)]
        release: bool,

        /// Emit CMakeLists.txt
        #[arg(long)]
        cmake: bool,

        /// Emit Makefile
        #[arg(long)]
        makefile: bool,

        /// Only transpile, skip cargo build
        #[arg(long)]
        transpile_only: bool,

        /// Output directory for transpiled Rust
        #[arg(long, default_value = ".vira-out")]
        out_dir: PathBuf,
    },

    /// Transpile a single .vira file
    Transpile {
        /// Input .vira file
        input: PathBuf,

        /// Output directory
        #[arg(short, long, default_value = ".vira-out")]
        out: PathBuf,

        /// Project name (used in Cargo.toml)
        #[arg(long)]
        name: Option<String>,

        /// Emit CMakeLists.txt
        #[arg(long)]
        cmake: bool,

        /// Emit Makefile
        #[arg(long)]
        makefile: bool,
    },

    /// Run a Vira project
    Run {
        /// Project directory
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Extra arguments to pass to the binary
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Check a Vira project (parse + type-check, no code gen)
    Check {
        /// Project directory
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Format Vira source files
    Fmt {
        /// Project directory or file
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Check formatting without modifying files
        #[arg(long)]
        check: bool,
    },

    /// Show the generated Rust code for a .vira file
    Show {
        /// Input .vira file
        input: PathBuf,
    },

    /// Display Vira version and toolchain info
    Version,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum Template {
    /// Tauri desktop app (default, main focus of Vira)
    Tauri,
    /// GTK desktop app
    Gtk,
    /// Qt desktop app
    Qt,
    /// CLI application (no GUI)
    Cli,
    /// Library
    Lib,
}

// ─── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("{} {e:#}", "error:".red().bold());
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::New { name, template } => {
            cmd_new(&name, template, cli.verbose)
        }
        Commands::Build {
            path, release, cmake, makefile, transpile_only, out_dir,
        } => {
            cmd_build(&path, release, cmake, makefile, transpile_only, &out_dir, cli.verbose)
        }
        Commands::Transpile { input, out, name, cmake, makefile } => {
            cmd_transpile(&input, &out, name.as_deref(), cmake, makefile, cli.verbose)
        }
        Commands::Run { path, args } => {
            cmd_run(&path, &args, cli.verbose)
        }
        Commands::Check { path } => {
            cmd_check(&path, cli.verbose)
        }
        Commands::Fmt { path, check } => {
            cmd_fmt(&path, check, cli.verbose)
        }
        Commands::Show { input } => {
            cmd_show(&input)
        }
        Commands::Version => {
            cmd_version();
            Ok(())
        }
    }
}

// ─── Commands ─────────────────────────────────────────────────────────────────

fn cmd_new(name: &str, template: Template, verbose: bool) -> Result<()> {
    println!("{} new Vira project: {}", "Creating".green().bold(), name.cyan().bold());
    project::create(name, template, verbose)
}

fn cmd_transpile(
    input: &Path,
    out: &Path,
    project_name: Option<&str>,
    emit_cmake: bool,
    emit_makefile: bool,
    verbose: bool,
) -> Result<()> {
    let name = project_name
        .map(|n| n.to_owned())
        .or_else(|| {
            input.file_stem().map(|s| s.to_string_lossy().to_string())
        })
        .unwrap_or_else(|| "vira_app".to_owned());

    println!(
        "{} {} → {}",
        "Transpiling".cyan().bold(),
        input.display().to_string().yellow(),
        out.display().to_string().yellow()
    );

    let source = std::fs::read_to_string(input)
        .with_context(|| format!("reading {}", input.display()))?;

    let result = vira_compiler::compile(&source, &name, emit_cmake, emit_makefile)
        .with_context(|| format!("compiling {}", input.display()))?;

    vira_compiler::write_output(&result, out)
        .with_context(|| format!("writing output to {}", out.display()))?;

    if verbose {
        println!("\n{}", "=== Generated Rust ===".dimmed());
        println!("{}", result.rust_source.dimmed());
    }

    println!("{} Transpiled to {}", "✓".green().bold(), out.display());
    Ok(())
}

fn cmd_build(
    path: &Path,
    release: bool,
    emit_cmake: bool,
    emit_makefile: bool,
    transpile_only: bool,
    out_dir: &Path,
    verbose: bool,
) -> Result<()> {
    // Find all .vira files in project
    let vira_files = collect_vira_files(path)?;
    if vira_files.is_empty() {
        anyhow::bail!("No .vira files found in {}", path.display());
    }

    // Read project manifest
    let project_name = read_project_name(path).unwrap_or_else(|| {
        path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "vira_app".to_owned())
    });

    println!(
        "{} {} ({})",
        "Building".green().bold(),
        project_name.cyan().bold(),
        if release { "release" } else { "debug" }
    );

    // Concatenate all .vira sources
    let mut combined_source = String::new();
    for f in &vira_files {
        if verbose {
            println!("  {} {}", "→".dimmed(), f.display());
        }
        combined_source.push_str(&std::fs::read_to_string(f)
            .with_context(|| format!("reading {}", f.display()))?);
        combined_source.push('\n');
    }

    // Transpile
    let out = out_dir;
    let result = vira_compiler::compile(&combined_source, &project_name, emit_cmake, emit_makefile)
        .context("Vira compilation failed")?;
    vira_compiler::write_output(&result, out)
        .context("writing transpile output")?;

    println!("{} Transpiled {} file(s) → {}", "✓".green().bold(), vira_files.len(), out.display());

    if transpile_only {
        return Ok(());
    }

    // cargo build
    let mut cmd = Command::new("cargo");
    cmd.arg("build")
        .arg("--manifest-path")
        .arg(out.join("Cargo.toml"));
    if release {
        cmd.arg("--release");
    }
    if verbose {
        cmd.arg("--verbose");
    }

    println!("{} cargo build...", "Running".cyan().bold());
    let status = cmd.status().context("running cargo build")?;
    if !status.success() {
        anyhow::bail!("cargo build failed with status: {status}");
    }

    println!("{} Build complete.", "✓".green().bold());
    Ok(())
}

fn cmd_run(path: &Path, extra_args: &[String], verbose: bool) -> Result<()> {
    let out_dir = path.join(".vira-out");
    cmd_build(path, false, false, false, false, &out_dir, verbose)?;

    let project_name = read_project_name(path).unwrap_or_else(|| "vira_app".to_owned());
    let binary = out_dir.join(format!("target/debug/{project_name}"));

    println!("{} {}", "Running".green().bold(), binary.display());
    let mut cmd = Command::new(&binary);
    cmd.args(extra_args);
    let status = cmd.status().with_context(|| format!("running {}", binary.display()))?;

    if !status.success() {
        anyhow::bail!("program exited with status: {status}");
    }
    Ok(())
}

fn cmd_check(path: &Path, verbose: bool) -> Result<()> {
    let vira_files = collect_vira_files(path)?;
    let mut errors = 0usize;

    println!("{} {} file(s)...", "Checking".cyan().bold(), vira_files.len());

    for file in &vira_files {
        let source = std::fs::read_to_string(file)
            .with_context(|| format!("reading {}", file.display()))?;

        match vira_parser::parse(&source) {
            Ok(ast) => {
                if verbose {
                    println!("  {} {} ({} items)", "✓".green(), file.display(), ast.items.len());
                }
            }
            Err(e) => {
                eprintln!("  {} {} — {e}", "✗".red().bold(), file.display());
                errors += 1;
            }
        }
    }

    if errors == 0 {
        println!("{} All files OK", "✓".green().bold());
        Ok(())
    } else {
        anyhow::bail!("{errors} file(s) had errors");
    }
}

fn cmd_fmt(path: &Path, check_only: bool, _verbose: bool) -> Result<()> {
    let files = if path.is_file() {
        vec![path.to_path_buf()]
    } else {
        collect_vira_files(path)?
    };

    if check_only {
        println!("{} formatting (check only)...", "Checking".cyan().bold());
    } else {
        println!("{} {} file(s)...", "Formatting".cyan().bold(), files.len());
    }

    for file in &files {
        let source = std::fs::read_to_string(file)
            .with_context(|| format!("reading {}", file.display()))?;

        let formatted = fmt::format_source(&source);

        if check_only {
            if formatted != source {
                eprintln!("  {} {} needs formatting", "✗".red(), file.display());
            }
        } else {
            std::fs::write(file, &formatted)
                .with_context(|| format!("writing {}", file.display()))?;
            println!("  {} {}", "✓".green(), file.display());
        }
    }

    Ok(())
}

fn cmd_show(input: &Path) -> Result<()> {
    let source = std::fs::read_to_string(input)
        .with_context(|| format!("reading {}", input.display()))?;

    let name = input.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "vira_app".to_owned());

    let result = vira_compiler::compile(&source, &name, false, false)
        .context("compilation failed")?;

    println!("{}", "=== Generated Rust ===".cyan().bold());
    println!("{}", result.rust_source);
    println!("{}", "=== Cargo.toml ===".cyan().bold());
    println!("{}", result.cargo_toml);

    Ok(())
}

fn cmd_version() {
    println!(
        "{} {} — Vira transpiler to Rust",
        "vira".cyan().bold(),
        env!("CARGO_PKG_VERSION").yellow()
    );
    println!("HackerOS ecosystem: Tauri • GTK • Qt");
    println!("Memory model: Chained Arena");
    println!("Target: Rust {}", rustc_version());
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn collect_vira_files(dir: &Path) -> Result<Vec<PathBuf>> {
    if dir.is_file() && dir.extension().map_or(false, |e| e == "vira") {
        return Ok(vec![dir.to_path_buf()]);
    }

    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "vira") {
            // Skip hidden dirs and .vira-out
            let skip = path.components().any(|c| {
                let s = c.as_os_str().to_string_lossy();
                s.starts_with('.') || s == "target"
            });
            if !skip {
                files.push(path.to_path_buf());
            }
        }
    }
    files.sort();
    Ok(files)
}

fn read_project_name(dir: &Path) -> Option<String> {
    let manifest = dir.join("vira.toml");
    let content = std::fs::read_to_string(manifest).ok()?;
    let val: toml::Value = content.parse().ok()?;
    val.get("package")?.get("name")?.as_str().map(|s| s.to_owned())
}

fn rustc_version() -> String {
    Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_else(|| "unknown".to_owned())
        .trim()
        .to_owned()
}
