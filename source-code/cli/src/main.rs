use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use std::process::Command;

mod fmt;
mod hk;
mod project;
mod repl;
mod ui;
mod workspace;

// ─── Manifest ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct ViraManifest {
    name: String,
    version: String,
    entry: PathBuf,
    base: PathBuf,
    target: BuildTarget,
    window_title: Option<String>,
    window_width: Option<u32>,
    window_height: Option<u32>,
    /// Legacy html frontend (deprecated — use lib/)
    frontend: Option<PathBuf>,
    /// lib/ directory — UI written in .vira or .html
    lib_dir: Option<PathBuf>,
    /// icons/ directory
    icons_dir: Option<PathBuf>,
    /// manifest.toml overrides
    manifest_toml: Option<PathBuf>,
    /// Whether app needs icons (from manifest.toml)
    needs_icons: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum BuildTarget { Tauri, Gtk, Qt, Cli, Lib }

impl ViraManifest {
    fn load(path: &Path) -> Result<Self> {
        // path can be: a dir with vira.toml, OR a .vira file (look for vira.toml in parent)
        let base = if path.is_file() {
            path.parent().unwrap_or(path).to_path_buf()
        } else {
            path.to_path_buf()
        };

        // Accept vira.hk (native HK format) or vira.toml (legacy TOML)
        let hk_path   = base.join("vira.hk");
        let toml_path = base.join("_vira.toml.legacy"); // vira.toml no longer supported

        let (name, version, entry_rel, target, window_title, window_width, window_height, frontend) =
        if hk_path.exists() {
            // ── Parse native .hk format ──────────────────────────────────
            let content = std::fs::read_to_string(&hk_path)
            .with_context(|| format!("Cannot read {}", hk_path.display()))?;
            let doc = hk::parse_hk(&content)
            .map_err(|e| anyhow::anyhow!("Invalid vira.hk: {e}"))?;

            let name    = hk::get_str(&doc, "package", "name")
            .context("vira.hk: [package] -> name is required")?.to_owned();
            let version = hk::get_str(&doc, "package", "version")
            .unwrap_or("0.1.0").to_owned();
            let entry   = hk::get_str(&doc, "build", "entry")
            .unwrap_or("src/main.vira").to_owned();
            let tgt = match hk::get_str(&doc, "build", "target").unwrap_or("cli") {
                "tauri" => BuildTarget::Tauri, "gtk" => BuildTarget::Gtk,
                "qt"    => BuildTarget::Qt,    "lib" => BuildTarget::Lib,
                _       => BuildTarget::Cli,
            };
            let wt = hk::get_str(&doc, "tauri", "window_title").map(|s| s.to_owned());
            let ww = hk::get_f64(&doc, "tauri", "window_width").map(|n| n as u32);
            let wh = hk::get_f64(&doc, "tauri", "window_height").map(|n| n as u32);
            let fe = hk::get_str(&doc, "tauri", "frontend").map(|f| base.join(f));
            (name, version, entry, tgt, wt, ww, wh, fe)
        } else if toml_path.exists() { // legacy — should not happen
            // ── Legacy .toml format (DEPRECATED — use vira.hk) ──────────────────────────────────────
            let content = std::fs::read_to_string(&toml_path)
            .with_context(|| format!("Cannot read {}", toml_path.display()))?;
            let val: toml::Value = content.parse().context("Invalid vira.toml")?;
            let pkg = val.get("package").context("vira.toml: missing [package]")?;
            let name    = pkg.get("name").and_then(|v| v.as_str())
            .context("vira.toml: [package].name required")?.to_owned();
            let version = pkg.get("version").and_then(|v| v.as_str()).unwrap_or("0.1.0").to_owned();
            let bs  = val.get("build");
            let entry   = bs.and_then(|b| b.get("entry")).and_then(|v| v.as_str()).unwrap_or("src/main.vira").to_owned();
            let tgt = match bs.and_then(|b| b.get("target")).and_then(|v| v.as_str()).unwrap_or("cli") {
                "tauri" => BuildTarget::Tauri, "gtk" => BuildTarget::Gtk,
                "qt"    => BuildTarget::Qt,    "lib" => BuildTarget::Lib,
                _       => BuildTarget::Cli,
            };
            let t   = val.get("tauri");
            let wt  = t.and_then(|t| t.get("window_title")).and_then(|v| v.as_str()).map(|s| s.to_owned());
            let ww  = t.and_then(|t| t.get("window_width")).and_then(|v| v.as_integer()).map(|n| n as u32);
            let wh  = t.and_then(|t| t.get("window_height")).and_then(|v| v.as_integer()).map(|n| n as u32);
            let fe  = t.and_then(|t| t.get("frontend")).and_then(|v| v.as_str()).map(|f| base.join(f));
            (name, version, entry, tgt, wt, ww, wh, fe)
        } else {
            anyhow::bail!(
                "Cannot find vira.hk or vira.toml in {}\n  Run `vira new <name>` to create a project.",
                base.display()
            )
        };

        let entry_rel = entry_rel;

        // lib/ — new UI directory
        let lib_dir = if base.join("lib").exists() {
            Some(base.join("lib"))
        } else { None };

        // icons/
        let icons_dir = if base.join("icons").exists() {
            Some(base.join("icons"))
        } else { None };

        // manifest.toml — read if present, overrides some fields
        let manifest_toml_path = base.join("manifest.toml");
        let (manifest_toml, window_title, window_width, window_height, needs_icons) =
        if manifest_toml_path.exists() {
            let mt: toml::Value = std::fs::read_to_string(&manifest_toml_path)
            .ok().and_then(|s| s.parse().ok()).unwrap_or(toml::Value::Table(Default::default()));
            let wt = mt.get("window").and_then(|w| w.get("title")).and_then(|v| v.as_str())
            .map(|s| s.to_owned()).or(window_title);
            let ww = mt.get("window").and_then(|w| w.get("width")).and_then(|v| v.as_integer())
            .map(|n| n as u32).or(window_width);
            let wh = mt.get("window").and_then(|w| w.get("height")).and_then(|v| v.as_integer())
            .map(|n| n as u32).or(window_height);
            let ni = mt.get("icons").and_then(|v| v.as_bool()).unwrap_or(true);
            (Some(manifest_toml_path), wt, ww, wh, ni)
        } else {
            (None, window_title, window_width, window_height, icons_dir.is_some())
        };

        Ok(ViraManifest {
            name, version, target, base: base.clone(),
           entry: base.join(entry_rel),
           window_title, window_width, window_height, frontend,
           lib_dir, icons_dir, manifest_toml, needs_icons,
        })
    }
}

// ─── CLI definition ───────────────────────────────────────────────────────────

/// Vira language toolchain — transpiled to Rust, HackerOS native.
#[derive(Parser)]
#[command(
name = "vira",
author = "HackerOS Team",
version = env!("CARGO_PKG_VERSION"),
          about = "Vira — Tauri · GTK · Qt, transpiled to Rust"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new Vira project
    New {
        name: String,
        #[arg(long, default_value = "tauri")] template: Template,
    },

    /// Build project
    ///
    /// Examples:
    ///   vira build                    debug build from vira.hk
    ///   vira build --production       release + installer bundle
    ///   vira build --workspace        build all workspace members
    ///   vira build --lib              build as library
    ///   vira build --staticlib        build as static library (.a)
    ///   vira build --shared           build as shared library (.so)
    ///   vira build --rustlib          build as Rust library (.rlib)
    ///   vira build --viralib          build as Vira library (.vlib)
    Build {
        /// Project directory (with vira.hk) OR single .vira file
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Full production release build + installer bundle
        #[arg(long)]
        production: bool,

        /// Alias for --production
        #[arg(long, short)]
        release: bool,

        /// Build all workspace members (project.hk)
        #[arg(long)]
        workspace: bool,

        /// Build as library (rlib + cdylib)
        #[arg(long)]
        lib: bool,

        /// Build as static library (.a)
        #[arg(long)]
        staticlib: bool,

        /// Build as shared library (.so / .dylib)
        #[arg(long)]
        shared: bool,

        /// Build as Rust library (.rlib)
        #[arg(long)]
        rustlib: bool,

        /// Build as Vira library (.vlib for vira.io)
        #[arg(long)]
        viralib: bool,

        /// Transpile only, skip cargo build
        #[arg(long)]
        transpile_only: bool,

        /// Emit CMakeLists.txt
        #[arg(long)] cmake: bool,
        /// Emit Makefile
        #[arg(long)] makefile: bool,
        /// Override output directory
        #[arg(long)] out_dir: Option<PathBuf>,
    },

    /// Transpile a single .vira file to Rust without building
    Transpile {
        input: PathBuf,
        #[arg(short, long, default_value = ".vira-out")] out: PathBuf,
        #[arg(long)] name: Option<String>,
        #[arg(long)] cmake: bool,
        #[arg(long)] makefile: bool,
    },

    /// Build and immediately run the project
    Run {
        #[arg(default_value = ".")] path: PathBuf,
        #[arg(trailing_var_arg = true)] args: Vec<String>,
    },

    /// Check syntax without generating code
    Check {
        #[arg(default_value = ".")] path: PathBuf,
    },

    /// Format .vira source files
    Fmt {
        #[arg(default_value = ".")] path: PathBuf,
        #[arg(long)] check: bool,
    },

    /// Print generated Rust source for a .vira file
    Show { input: PathBuf },

    /// Create a new workspace
    WorkspaceNew {
        name: String,
    },

    /// Print toolchain version
    Version,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum Template { Tauri, Gtk, Qt, Cli, Lib }

// ─── Main ─────────────────────────────────────────────────────────────────────


// ─── Build root resolution ────────────────────────────────────────────────────
// build/ is always LOCAL to the project — right next to src/ and vira.toml
//
//   my-project/
//   ├── src/main.vira
//   ├── vira.toml
//   └── build/
//       ├── cache/   ← transpiled Rust + cargo artifacts
//       └── my-app   ← final binary
//
// Override with VIRA_BUILD_DIR env var if needed.

fn vira_build_root_for(project_base: &std::path::Path) -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("VIRA_BUILD_DIR") {
        let p = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&p).ok();
        return p;
    }
    let local = project_base.join("build");
    std::fs::create_dir_all(&local).ok();
    local
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("\n{} {e:#}\n", "✗ error:".red().bold());
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::New { name, template } =>
        cmd_new(&name, template, cli.verbose),
        Commands::Build { path, production, release, workspace, lib, staticlib, shared, rustlib, viralib, transpile_only, cmake, makefile, out_dir } => {
            if workspace {
                cmd_build_ws(&path, production || release, cli.verbose)
            } else if lib || staticlib || shared || rustlib || viralib {
                let lib_type = if staticlib { LibType::Static }
                else if shared   { LibType::Shared }
                else if rustlib  { LibType::Rlib }
                else if viralib  { LibType::ViraLib }
                else             { LibType::Lib };
                cmd_build_lib(&path, lib_type, production || release, out_dir.as_deref(), cli.verbose)
            } else {
                cmd_build(&path, production || release, transpile_only, cmake, makefile, out_dir.as_deref(), cli.verbose)
            }
        }
        Commands::Transpile { input, out, name, cmake, makefile } =>
        cmd_transpile(&input, &out, name.as_deref(), cmake, makefile, cli.verbose),
        Commands::Run { path, args } =>
        cmd_run(&path, &args, cli.verbose),
        Commands::Check { path } =>
        cmd_check(&path, cli.verbose),
        Commands::Fmt { path, check } =>
        cmd_fmt(&path, check, cli.verbose),
        Commands::Show { input } =>
        cmd_show(&input),
        Commands::WorkspaceNew { name } =>
        workspace::create_workspace(&name),
        Commands::Version => { cmd_version(); Ok(()) }
    }
}

// ─── cmd_build ────────────────────────────────────────────────────────────────

fn cmd_build(
    path: &Path,
    release: bool,
    transpile_only: bool,
    emit_cmake: bool,
    emit_makefile: bool,
    out_dir_override: Option<&Path>,
    verbose: bool,
) -> Result<()> {
    // Allow: vira build src/main.vira  (single file, no vira.toml needed)
    if path.is_file() && path.extension().map_or(false, |e| e == "vira") {
        let name = path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "vira_app".into());
        let file_base = path.parent().unwrap_or(Path::new("."));
        let out = out_dir_override
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| {
            vira_build_root_for(file_base)
            .join("cache").join(&name).join("rust-src")
        });
        return cmd_transpile_and_build(path, &out, &name, release, transpile_only, emit_cmake, emit_makefile, verbose);
    }

    // Standard: directory with vira.toml
    let manifest = ViraManifest::load(path)?;
    // Default output layout:
    //   Transpiled Rust: <project>/build/cache/<name>/rust-src/
    //   Final binary:   <project>/build/<name>
    // Override with VIRA_BUILD_DIR env var or --out-dir flag
    let out_dir = out_dir_override.map(|p| p.to_path_buf())
    .unwrap_or_else(|| {
        vira_build_root_for(&manifest.base)
        .join("cache").join(&manifest.name).join("rust-src")
    });

    print_build_header(&manifest, release);

    // Collect sources
    let vira_files = collect_vira_files(&manifest.base)?;
    if vira_files.is_empty() {
        anyhow::bail!("No .vira files found in {}", manifest.base.display());
    }

    // ── Transpile with spinner ────────────────────────────────────────────────
    let spinner = ui::transpile_spinner(&format!(
        "Transpiling {} — {} file(s)...",
                                                 manifest.name.bold(),
                                                 vira_files.len()
    ));

    let mut source = String::new();
    for f in &vira_files {
        source.push_str(&std::fs::read_to_string(f)
        .with_context(|| format!("reading {}", f.display()))?);
        source.push('\n');
    }

    let result = vira_compiler::compile(&source, &manifest.name, emit_cmake, emit_makefile)
    .map_err(|e| { spinner.finish_and_clear(); e })
    .context("Transpilation failed")?;

    vira_compiler::write_output(&result, &out_dir)
    .map_err(|e| { spinner.finish_and_clear(); e })
    .context("Writing output failed")?;

    spinner.finish_and_clear();
    print_ok(&format!("Transpiled → {}", out_dir.join("src/main.rs").display()));

    if transpile_only {
        println!("\n{} Done (transpile only)\n", "✓".green().bold());
        return Ok(());
    }

    // ── Tauri assets ──────────────────────────────────────────────────────────
    if manifest.target == BuildTarget::Tauri {
        copy_tauri_assets(&manifest.base, &out_dir, &manifest)?;
    }

    // ── Compile ───────────────────────────────────────────────────────────────
    match (&manifest.target, release) {
        (BuildTarget::Tauri, true)  => build_tauri_production(&manifest, &out_dir),
        (BuildTarget::Tauri, false) => build_tauri_dev(&manifest, &out_dir, verbose),
        _                           => build_cargo_bar(&manifest, &out_dir, release),
    }
}

// Single-file build (no vira.toml)
fn cmd_transpile_and_build(
    input: &Path, out: &Path, name: &str,
    release: bool, transpile_only: bool,
    emit_cmake: bool, emit_makefile: bool, _verbose: bool,
) -> Result<()> {
    println!("\n{} {} (single file)\n", "Building".green().bold(), name.cyan().bold());

    let spinner = ui::transpile_spinner(&format!("Transpiling {}...", input.display()));
    let source  = std::fs::read_to_string(input)
    .with_context(|| format!("reading {}", input.display()))?;
    let result  = vira_compiler::compile(&source, name, emit_cmake, emit_makefile)
    .map_err(|e| { spinner.finish_and_clear(); e })
    .context("Transpilation failed")?;
    vira_compiler::write_output(&result, out)
    .map_err(|e| { spinner.finish_and_clear(); e })?;

    spinner.finish_and_clear();
    print_ok(&format!("Transpiled → {}", out.display()));

    if transpile_only { return Ok(()); }

    let fake_manifest = ViraManifest {
        name: name.to_owned(), version: "0.1.0".into(),
        entry: input.to_path_buf(),
        base: input.parent().unwrap_or(Path::new(".")).to_path_buf(),
        target: BuildTarget::Cli,
        window_title: None, window_width: None, window_height: None,
        frontend: None, lib_dir: None, icons_dir: None,
        manifest_toml: None, needs_icons: false,
    };
    build_cargo_bar(&fake_manifest, out, release)
}

// ─── Cargo build with progress bar ───────────────────────────────────────────

fn build_cargo_bar(manifest: &ViraManifest, out_dir: &Path, release: bool) -> Result<()> {
    println!("  {} {}", "◈".cyan(), "Compiling with Rust".bold());
    println!();

    let mut args = vec!["build"];
    if release { args.push("--release"); }

    ui::run_cargo_with_progress(
        &args,
        &out_dir.join("Cargo.toml"),
                                &manifest.name,
    )?;

    let prof = if release { "release" } else { "debug" };
    // Cargo puts the binary in out_dir/target/<profile>/<name>
    // We also copy it to build/<name>/ for easy access
    let built_binary = out_dir.join(format!("target/{}/{}", prof, manifest.name));

    let build_root   = vira_build_root_for(&manifest.base);
    let final_dir    = build_root.join(&manifest.name);
    let final_binary = final_dir.join(&manifest.name);

    // Determine which path actually exists and use that
    let runnable = if built_binary.exists() {
        // Try to copy to build/<name>/ for convenience
        if std::fs::create_dir_all(&final_dir).is_ok() {
            if std::fs::copy(&built_binary, &final_binary).is_ok() {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(meta) = std::fs::metadata(&final_binary) {
                        let mut perms = meta.permissions();
                        perms.set_mode(0o755);
                        let _ = std::fs::set_permissions(&final_binary, perms);
                    }
                }
                final_binary.clone()
            } else {
                built_binary.clone()
            }
        } else {
            built_binary.clone()
        }
    } else {
        // built_binary not found — search in target dir
        let target_dir = out_dir.join("target");
        let candidates = [
            target_dir.join(format!("{}/{}", prof, manifest.name)),
            target_dir.join(&manifest.name),
            built_binary.clone(),
        ];
        candidates.iter().find(|p| p.exists())
        .cloned()
        .unwrap_or(built_binary.clone())
    };

    println!();
    if runnable.exists() {
        print_ok(&format!("Binary → {}", runnable.display()));
        println!();
        println!("  {} Run:  {}", "›".cyan(), runnable.display().to_string().cyan());
    } else {
        // Build succeeded but binary location is unexpected
        print_ok("Build complete (binary location unknown)");
        println!("  {} Check: {}", "›".cyan(), out_dir.join(format!("target/{}", prof)).display());
    }
    println!();
    Ok(())
}

// ─── Tauri dev ────────────────────────────────────────────────────────────────

fn build_tauri_dev(manifest: &ViraManifest, out_dir: &Path, _verbose: bool) -> Result<()> {
    if !tauri_cli_ok() {
        println!("{}", "\n  tauri-cli not found — falling back to cargo build".yellow());
        println!("  {} cargo install tauri-cli\n", "Install:".cyan());
        return build_cargo_bar(manifest, out_dir, false);
    }

    println!("  {} {}\n", "◈".cyan(), "Starting Tauri dev server".bold());

    // tauri dev streams output interactively — just run it directly
    let status = Command::new("cargo")
    .args(["tauri", "dev"])
    .current_dir(out_dir)
    .status()
    .context("cargo tauri dev failed")?;

    if !status.success() { anyhow::bail!("tauri dev failed"); }
    Ok(())
}

// ─── Tauri production build ───────────────────────────────────────────────────

fn build_tauri_production(manifest: &ViraManifest, out_dir: &Path) -> Result<()> {
    println!("  {} {}\n", "◈".cyan().bold(), "Production build — bundling installer".bold());

    if !tauri_cli_ok() {
        println!("{}", "  tauri-cli not found — building release binary only".yellow());
        println!("  {} cargo install tauri-cli\n", "Install:".cyan());
        return build_cargo_bar(manifest, out_dir, true);
    }

    ui::run_tauri_with_progress(out_dir, true, &manifest.name)?;

    let bundle_dir = out_dir.join("target/release/bundle");
    println!();
    print_ok("Bundle created");
    println!();
    println!("  {} Installer: {}", "›".cyan(), bundle_dir.display().to_string().yellow());
    println!();
    Ok(())
}

// ─── Tauri assets ─────────────────────────────────────────────────────────────

fn copy_tauri_assets(_base: &Path, out_dir: &Path, manifest: &ViraManifest) -> Result<()> {
    // Always regenerate tauri.conf.json from manifest.toml / vira.toml
    // (user should NOT hand-edit tauri.conf.json — edit manifest.toml instead)
    let tauri_conf = gen_tauri_conf(manifest);
    std::fs::write(out_dir.join("tauri.conf.json"), &tauri_conf)
    .context("writing tauri.conf.json")?;
    print_ok("tauri.conf.json (Tauri v2, from manifest.toml)");

    // Copy lib/ → out/lib (new UI directory)
    if let Some(lib) = &manifest.lib_dir {
        if lib.exists() {
            copy_dir(lib, &out_dir.join("lib"))?;
            print_ok(&format!("lib/ → {}", out_dir.join("lib").display()));
        }
    } else if let Some(front) = &manifest.frontend {
        // Legacy: copy frontend/ if lib/ not present
        if front.exists() {
            copy_dir(front, &out_dir.join("lib"))?;
            print_ok(&format!("frontend/ → lib/ (legacy)"));
        }
    }

    // Copy icons/ if needed
    if manifest.needs_icons {
        if let Some(icons) = &manifest.icons_dir {
            if icons.exists() {
                copy_dir(icons, &out_dir.join("icons"))?;
                print_ok("icons/");
            } else {
                generate_placeholder_icons(out_dir)?;
                print_ok("icons/ (generated placeholder)");
            }
        } else {
            generate_placeholder_icons(out_dir)?;
            print_ok("icons/ (generated placeholder — Tauri requires icons)");
        }
    }

    Ok(())
}

/// Generate minimal placeholder PNG icons that satisfy Tauri's bundler.
fn generate_placeholder_icons(out_dir: &Path) -> Result<()> {
    let icons_dir = out_dir.join("icons");
    std::fs::create_dir_all(&icons_dir)?;

    // Minimal 1x1 transparent PNG (valid PNG header + IHDR + IDAT + IEND)
    let minimal_png: &[u8] = &[
        0x89,0x50,0x4E,0x47,0x0D,0x0A,0x1A,0x0A, // PNG sig
        0x00,0x00,0x00,0x0D,0x49,0x48,0x44,0x52, // IHDR length+type
        0x00,0x00,0x00,0x01,0x00,0x00,0x00,0x01, // 1x1
        0x08,0x06,0x00,0x00,0x00,0x1F,0x15,0xC4, // RGBA, CRC
        0x89,0x00,0x00,0x00,0x0B,0x49,0x44,0x41, // IDAT
        0x54,0x78,0x9C,0x62,0x00,0x00,0x00,0x02, // compressed data
        0x00,0x01,0xE2,0x21,0xBC,0x33,0x00,0x00, // IDAT end
        0x00,0x00,0x49,0x45,0x4E,0x44,0xAE,0x42, // IEND
        0x60,0x82,
    ];

    for name in &["32x32.png","128x128.png","128x128@2x.png"] {
        std::fs::write(icons_dir.join(name), minimal_png)?;
    }
    // .ico is just the PNG bytes for placeholder purposes
    std::fs::write(icons_dir.join("icon.ico"), minimal_png)?;
    // .icns placeholder
    std::fs::write(icons_dir.join("icon.icns"), minimal_png)?;

    Ok(())
}

fn gen_tauri_conf(m: &ViraManifest) -> String {
    let title  = m.window_title.as_deref().unwrap_or(&m.name);
    let width  = m.window_width.unwrap_or(1024);
    let height = m.window_height.unwrap_or(768);
    // lib/ is the new UI directory; fallback to frontend/
    let front  = m.lib_dir.as_ref().map(|f| f.display().to_string())
    .or_else(|| m.frontend.as_ref().map(|f| f.display().to_string()))
    .unwrap_or_else(|| "../lib".into());
    // Tauri v2 schema
    // frontendDist must be relative path WITHOUT ./ prefix (Tauri resolves from app root)
    // It should point to the DIRECTORY, not an HTML file
    let front_stripped = front
    .trim_start_matches("./")
    .trim_start_matches("../")
    .to_owned();
    // If it ends with .html or .htm, use parent directory
    let front_clean = if front_stripped.ends_with(".html") || front_stripped.ends_with(".htm") {
        std::path::Path::new(&front_stripped)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "lib".to_owned())
    } else {
        front_stripped
    };
    format!(
        r#"{{
        "$schema": "https://schema.tauri.app/config/2",
        "productName": "{t}",
        "version": "{v}",
        "identifier": "pl.hackeros.{n}",
        "build": {{
        "frontendDist": "{fc}"
}},
"app": {{
"windows": [{{
"title": "{t}",
"width": {w},
"height": {h},
"resizable": true
}}],
"security": {{
"csp": null
}}
}},
"bundle": {{
"active": true,
"targets": "all",
"icon": ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.icns", "icons/icon.ico"]
}}
}}"#,
fc=front_clean, t=title, v=m.version, n=m.name, h=height, w=width
    )
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        if e.file_type()?.is_dir() { copy_dir(&e.path(), &dst.join(e.file_name()))?; }
        else { std::fs::copy(e.path(), dst.join(e.file_name()))?; }
    }
    Ok(())
}

fn tauri_cli_ok() -> bool {
    Command::new("cargo").args(["tauri","--version"])
    .output().map(|o| o.status.success()).unwrap_or(false)
}

// ─── cmd_transpile ────────────────────────────────────────────────────────────

fn cmd_transpile(
    input: &Path, out: &Path, project_name: Option<&str>,
    emit_cmake: bool, emit_makefile: bool, verbose: bool,
) -> Result<()> {
    let name = project_name.map(|s| s.to_owned())
    .or_else(|| input.file_stem().map(|s| s.to_string_lossy().into_owned()))
    .unwrap_or_else(|| "vira_app".into());

    let spinner = ui::transpile_spinner(&format!(
        "Transpiling {} → {}",
        input.display().to_string().yellow(),
                                                 out.display().to_string().yellow(),
    ));

    let source = std::fs::read_to_string(input)
    .with_context(|| format!("reading {}", input.display()))?;
    let result = vira_compiler::compile(&source, &name, emit_cmake, emit_makefile)
    .map_err(|e| { spinner.finish_and_clear(); e })
    .with_context(|| format!("compiling {}", input.display()))?;
    vira_compiler::write_output(&result, out)
    .map_err(|e| { spinner.finish_and_clear(); e })?;

    spinner.finish_and_clear();
    if verbose {
        println!("\n{}\n{}", "=== Generated Rust ===".dimmed(), result.rust_source.dimmed());
    }
    print_ok(&format!("→ {}/src/main.rs", out.display()));
    Ok(())
}

// ─── cmd_run ──────────────────────────────────────────────────────────────────

fn cmd_run(path: &Path, extra_args: &[String], verbose: bool) -> Result<()> {
    let base = if path.is_file() { path.parent().unwrap_or(path) } else { path };
    // Build first (uses build/cache/ layout)
    cmd_build(path, false, false, false, false, None, verbose)?;

    let manifest = ViraManifest::load(path)?;
    let out_dir  = vira_build_root_for(base)
    .join("cache").join(&manifest.name).join("rust-src");

    // Find binary: build/<name>/<name> → target/debug/<name>
    let build_root  = vira_build_root_for(base);
    let convenience = build_root.join(&manifest.name).join(&manifest.name);
    let cargo_debug = out_dir.join(format!("target/debug/{}", manifest.name));
    let cargo_rel   = out_dir.join(format!("target/release/{}", manifest.name));

    let binary = [convenience, cargo_debug, cargo_rel]
    .into_iter()
    .find(|p| p.exists())
    .ok_or_else(|| anyhow::anyhow!(
        "Binary not found after build. Looked in {}",
        build_root.join(&manifest.name).display()
    ))?;

    println!("{} {}", "Running".green().bold(), binary.display());
    let st = Command::new(&binary).args(extra_args).status()
    .with_context(|| format!("running {}", binary.display()))?;
    if !st.success() { anyhow::bail!("program exited with: {st}"); }
    Ok(())
}

// ─── cmd_check ────────────────────────────────────────────────────────────────

fn cmd_check(path: &Path, verbose: bool) -> Result<()> {
    let dir = if path.is_file() { path.parent().unwrap_or(path) } else { path };
    let files = collect_vira_files(dir)?;
    let bar = ProgressBar::new(files.len() as u64);
    bar.set_style(
        ProgressStyle::with_template("  {bar:30.cyan} {pos}/{len}  {msg}").unwrap()
        .progress_chars("█▉▊▋▌▍▎▏  "),
    );
    let mut errors = 0usize;
    for f in &files {
        bar.set_message(f.file_name().unwrap_or_default().to_string_lossy().to_string());
        let src = std::fs::read_to_string(f).with_context(|| format!("reading {}", f.display()))?;
        match vira_parser::parse(&src) {
            Ok(ast) => { if verbose { bar.println(format!("  {} {} ({} items)", "✓".green(), f.display(), ast.items.len())); } }
            Err(e)  => { bar.println(format!("  {} {} — {e}", "✗".red(), f.display())); errors += 1; }
        }
        bar.inc(1);
    }
    bar.finish_and_clear();
    if errors == 0 {
        println!("{} {} file(s) OK", "✓".green().bold(), files.len());
        Ok(())
    } else {
        anyhow::bail!("{errors} file(s) had errors")
    }
}

// ─── cmd_fmt ──────────────────────────────────────────────────────────────────

fn cmd_fmt(path: &Path, check_only: bool, _verbose: bool) -> Result<()> {
    let files = if path.is_file() && path.extension().map_or(false, |e| e == "vira") {
        vec![path.to_path_buf()]
    } else {
        let dir = if path.is_file() { path.parent().unwrap_or(path) } else { path };
        collect_vira_files(dir)?
    };
    let label = if check_only { "Checking" } else { "Formatting" };
    println!("{} {} file(s)...", label.cyan().bold(), files.len());
    for f in &files {
        let src = std::fs::read_to_string(f).with_context(|| format!("reading {}", f.display()))?;
        let out = fmt::format_source(&src);
        if check_only {
            if out != src { eprintln!("  {} {} needs formatting", "✗".red(), f.display()); }
        } else {
            std::fs::write(f, &out).with_context(|| format!("writing {}", f.display()))?;
            println!("  {} {}", "✓".green(), f.display());
        }
    }
    Ok(())
}

// ─── cmd_show ─────────────────────────────────────────────────────────────────

fn cmd_show(input: &Path) -> Result<()> {
    let src  = std::fs::read_to_string(input).with_context(|| format!("reading {}", input.display()))?;
    let name = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "app".into());
    let res  = vira_compiler::compile(&src, &name, false, false).context("compilation failed")?;
    println!("{}\n{}", "=== Generated Rust ===".cyan().bold(), res.rust_source);
    println!("{}\n{}", "=== Cargo.toml ===".cyan().bold(), res.cargo_toml);
    Ok(())
}

// ─── cmd_new ──────────────────────────────────────────────────────────────────

fn cmd_new(name: &str, template: Template, verbose: bool) -> Result<()> {
    let spinner = ui::transpile_spinner(&format!("Creating project {}...", name.cyan().bold()));
    let result  = project::create(name, template, verbose);
    spinner.finish_and_clear();
    result?;
    println!("{} Created {}", "✓".green().bold(), name.cyan().bold());
    println!();
    println!("  {} cd {}", "›".cyan(), name);
    println!("  {} vira build", "›".cyan());
    println!("  {} vira build --production", "›".cyan());
    println!();
    Ok(())
}

// ─── cmd_version ──────────────────────────────────────────────────────────────

fn cmd_version() {
    println!();
    println!("  {} {}  —  Vira transpiler", "vira".cyan().bold(), env!("CARGO_PKG_VERSION").yellow());
    println!("  HackerOS:  Tauri • GTK • Qt");
    println!("  Memory:    Chained Arena");
    println!("  Rust:      {}", rustc_version());
    println!();
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn print_build_header(m: &ViraManifest, release: bool) {
    let target = match m.target {
        BuildTarget::Tauri => "tauri", BuildTarget::Gtk => "gtk",
        BuildTarget::Qt    => "qt",    BuildTarget::Cli => "cli",
        BuildTarget::Lib   => "lib",
    };
    let mode = if release { "production" } else { "debug" };
    println!();
    println!("  {} {} v{}  {} {}",
             "Building".green().bold(),
             m.name.cyan().bold(),
             m.version.dimmed(),
             format!("[{target}]").yellow(),
                 format!("({mode})").dimmed(),
    );
    println!();
}

fn print_ok(msg: &str) {
    println!("  {} {}", "✓".green(), msg);
}


// ─── Library build types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum LibType {
    /// rlib + cdylib (default lib)
    Lib,
    /// staticlib (.a)
    Static,
    /// cdylib (.so / .dylib)
    Shared,
    /// rlib (.rlib)
    Rlib,
    /// Vira library (.vlib) for vira.io ecosystem
    ViraLib,
}

impl LibType {
    fn crate_types(&self) -> &'static str {
        match self {
            LibType::Lib     => r#""rlib", "cdylib""#,
            LibType::Static  => r#""staticlib""#,
            LibType::Shared  => r#""cdylib""#,
            LibType::Rlib    => r#""rlib""#,
            LibType::ViraLib => r#""rlib", "cdylib""#,
        }
    }
    fn ext(&self) -> &'static str {
        match self {
            LibType::Static  => ".a",
            LibType::Shared  => ".so",
            LibType::Rlib    => ".rlib",
            LibType::ViraLib => ".vlib",
            LibType::Lib     => ".rlib",
        }
    }
}

fn cmd_build_lib(
    path: &Path,
    lib_type: LibType,
    release: bool,
    out_dir_override: Option<&Path>,
    verbose: bool,
) -> Result<()> {
    let manifest = ViraManifest::load(path)?;
    let base = if path.is_file() { path.parent().unwrap_or(path) } else { path };
    let out_dir = out_dir_override.map(|p| p.to_path_buf())
    .unwrap_or_else(|| vira_build_root_for(base).join("cache").join(&manifest.name).join("rust-src"));

    println!("\n  {} {} v{}  [lib/{}]\n",
             "Building".green().bold(),
             manifest.name.cyan().bold(),
             manifest.version.dimmed(),
             lib_type.ext(),
    );

    let vira_files = collect_vira_files(base)?;
    if vira_files.is_empty() {
        anyhow::bail!("No .vira files found in {}", base.display());
    }

    let spinner = ui::transpile_spinner(&format!("Transpiling {} → library...", manifest.name.bold()));
    let mut source = String::new();
    for f in &vira_files {
        source.push_str(&std::fs::read_to_string(f)?);
        source.push('\n');
    }

    let result = vira_compiler::compile(&source, &manifest.name, false, false)
    .map_err(|e| { spinner.finish_and_clear(); e })
    .context("Transpilation failed")?;

    // Write lib output — change main.rs to lib.rs and add crate-type
    std::fs::create_dir_all(out_dir.join("src"))?;
    // Wrap in pub use / lib preamble
    let lib_source = format!(
        "// Generated Vira library: {name}\n         // Build type: {ext}\n         {stdlib}\n         {source}",
        name = manifest.name,
        ext = lib_type.ext(),
                             stdlib = vira_compiler::stdlib::STDLIB_PREAMBLE,
                             source = result.rust_source.replace(vira_compiler::stdlib::STDLIB_PREAMBLE, ""),
    );
    std::fs::write(out_dir.join("src/lib.rs"), &lib_source)?;

    // Write Cargo.toml with correct crate-type
    let mut cargo = result.cargo_toml.clone();
    let crate_types = lib_type.crate_types();
    cargo.push_str(&format!("\n[lib]\nname = \"{}\"\ncrate-type = [{crate_types}]\n", manifest.name.replace('-', "_")));
    std::fs::write(out_dir.join("Cargo.toml"), &cargo)?;

    spinner.finish_and_clear();
    print_ok(&format!("Transpiled → {}/src/lib.rs", out_dir.display()));

    println!("  {} {}  {}", "›".cyan(), "Compiling".bold(), format!("cargo build {} ({})", if release { "--release" } else { "" }, lib_type.ext()).dimmed());
    let status = std::process::Command::new("cargo")
    .arg("build")
    .arg("--manifest-path").arg(out_dir.join("Cargo.toml"))
    .args(if release { vec!["--release"] } else { vec![] })
    .status()
    .context("cargo not found")?;

    if !status.success() { anyhow::bail!("cargo build failed"); }

    let prof = if release { "release" } else { "debug" };
    let lib_name = format!("lib{}{}", manifest.name.replace('-', "_"), lib_type.ext());
    let built = out_dir.join(format!("target/{prof}/{lib_name}"));
    let dest_dir = vira_build_root_for(base).join(&manifest.name);
    std::fs::create_dir_all(&dest_dir)?;
    if built.exists() {
        std::fs::copy(&built, dest_dir.join(&lib_name))?;
        print_ok(&format!("Library → {}", dest_dir.join(&lib_name).display()));
    }

    if lib_type == LibType::ViraLib {
        println!();
        println!("  {} .vlib is ready for the Vira registry (vira.io — coming soon)", "◈".cyan());
    }
    println!();
    Ok(())
}

fn cmd_build_ws(path: &Path, release: bool, verbose: bool) -> Result<()> {
    if workspace::Workspace::is_workspace(path) {
        let ws = workspace::Workspace::load(path)?;
        ws.build_all(release, verbose)
    } else {
        anyhow::bail!(
            "No project.hk found in {}
            Use `vira workspace-new <name>` to create a workspace.",
            path.display()
        )
    }
}

pub fn cmd_build_member(path: &Path, release: bool, verbose: bool) -> Result<()> {
    cmd_build(path, release, false, false, false, None, verbose)
}

fn collect_vira_files(dir: &Path) -> Result<Vec<PathBuf>> {
    if dir.is_file() && dir.extension().map_or(false, |e| e == "vira") {
        return Ok(vec![dir.to_path_buf()]);
    }
    let mut files = Vec::new();
    for e in walkdir::WalkDir::new(dir).follow_links(true).into_iter().filter_map(|e| e.ok()) {
        let p = e.path();
        if p.extension().map_or(false, |ext| ext == "vira") {
            let skip = p.components().any(|c| {
                use std::path::Component;
                match c {
                    Component::Normal(s) => {
                        let s = s.to_string_lossy();
                        // skip .vira-out, .git, target — but NOT "." (CurDir)
                        (s.starts_with('.') && s.len() > 1) || s == "target"
                    }
                    _ => false,
                }
            });
            if !skip { files.push(p.to_path_buf()); }
        }
    }
    files.sort();
    Ok(files)
}

fn rustc_version() -> String {
    Command::new("rustc").arg("--version").output()
    .ok().and_then(|o| String::from_utf8(o.stdout).ok())
    .unwrap_or_else(|| "not found".into())
    .trim().to_owned()
}
