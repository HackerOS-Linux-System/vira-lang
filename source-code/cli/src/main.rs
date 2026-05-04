use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

mod fmt;
mod project;
mod repl;
mod ui;

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
    frontend: Option<PathBuf>,
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

        let manifest_path = base.join("vira.toml");
        let content = std::fs::read_to_string(&manifest_path)
        .with_context(|| format!(
            "Cannot find vira.toml in {}\n  Hint: run `vira new <name>` to create a project.",
            base.display()
        ))?;

        let val: toml::Value = content.parse().context("Invalid vira.toml — check TOML syntax")?;

        let pkg = val.get("package").context("vira.toml: missing [package] section")?;
        let name = pkg.get("name").and_then(|v| v.as_str())
        .context("vira.toml: [package].name is required")?.to_owned();
        let version = pkg.get("version").and_then(|v| v.as_str())
        .unwrap_or("0.1.0").to_owned();

        let build_sec = val.get("build");
        let entry_rel = build_sec.and_then(|b| b.get("entry")).and_then(|v| v.as_str())
        .unwrap_or("src/main.vira");
        let target = match build_sec.and_then(|b| b.get("target")).and_then(|v| v.as_str()).unwrap_or("cli") {
            "tauri" => BuildTarget::Tauri,
            "gtk"   => BuildTarget::Gtk,
            "qt"    => BuildTarget::Qt,
            "lib"   => BuildTarget::Lib,
            _       => BuildTarget::Cli,
        };

        let tauri = val.get("tauri");
        let window_title  = tauri.and_then(|t| t.get("window_title")).and_then(|v| v.as_str()).map(|s| s.to_owned());
        let window_width  = tauri.and_then(|t| t.get("window_width")).and_then(|v| v.as_integer()).map(|n| n as u32);
        let window_height = tauri.and_then(|t| t.get("window_height")).and_then(|v| v.as_integer()).map(|n| n as u32);
        let frontend      = tauri.and_then(|t| t.get("frontend")).and_then(|v| v.as_str())
        .map(|f| base.join(f));

        Ok(ViraManifest {
            name, version, target, base: base.clone(),
           entry: base.join(entry_rel),
           window_title, window_width, window_height, frontend,
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
    /// Usage:
    ///   vira build                  — debug build from vira.toml in current dir
    ///   vira build path/to/dir      — debug build from a vira.toml in that dir
    ///   vira build src/main.vira    — transpile + build a single .vira file
    ///   vira build --production     — full release + Tauri installer bundle
    Build {
        /// Project directory (with vira.toml) OR a single .vira file
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Full production release build + installer bundle
        #[arg(long)]
        production: bool,

        /// Alias for --production
        #[arg(long, short)]
        release: bool,

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

    /// Print toolchain version
    Version,
}

#[derive(clap::ValueEnum, Clone, Debug)]
enum Template { Tauri, Gtk, Qt, Cli, Lib }

// ─── Main ─────────────────────────────────────────────────────────────────────

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
        Commands::Build { path, production, release, transpile_only, cmake, makefile, out_dir } =>
        cmd_build(&path, production || release, transpile_only, cmake, makefile, out_dir.as_deref(), cli.verbose),
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
        let out = out_dir_override
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| {
            path.parent().unwrap_or(Path::new(".")).join(".vira-out")
        });
        let name = path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "vira_app".into());
        return cmd_transpile_and_build(path, &out, &name, release, transpile_only, emit_cmake, emit_makefile, verbose);
    }

    // Standard: directory with vira.toml
    let manifest = ViraManifest::load(path)?;
    let out_dir = out_dir_override.map(|p| p.to_path_buf())
    .unwrap_or_else(|| manifest.base.join(".vira-out"));

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
    emit_cmake: bool, emit_makefile: bool, verbose: bool,
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
        entry: input.to_path_buf(), base: input.parent().unwrap_or(Path::new(".")).to_path_buf(),
        target: BuildTarget::Cli,
        window_title: None, window_width: None, window_height: None, frontend: None,
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
    let binary = out_dir.join(format!("target/{}/{}", prof, manifest.name));
    println!();
    print_ok(&format!("Binary → {}", binary.display()));
    println!();
    println!("  {} Run with:  {}", "›".cyan(), format!("vira run .").cyan());
    println!();
    Ok(())
}

// ─── Tauri dev ────────────────────────────────────────────────────────────────

fn build_tauri_dev(manifest: &ViraManifest, out_dir: &Path, verbose: bool) -> Result<()> {
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

fn copy_tauri_assets(base: &Path, out_dir: &Path, manifest: &ViraManifest) -> Result<()> {
    let conf = base.join("tauri.conf.json");
    if conf.exists() {
        std::fs::copy(&conf, out_dir.join("tauri.conf.json")).context("copying tauri.conf.json")?;
        print_ok("tauri.conf.json");
    } else {
        std::fs::write(out_dir.join("tauri.conf.json"), gen_tauri_conf(manifest))
        .context("generating tauri.conf.json")?;
        print_ok("tauri.conf.json (generated from vira.toml)");
    }
    if let Some(front) = &manifest.frontend {
        if front.exists() {
            let dest = out_dir.join("frontend");
            copy_dir(front, &dest)?;
            print_ok(&format!("frontend → {}", dest.display()));
        }
    }
    Ok(())
}

fn gen_tauri_conf(m: &ViraManifest) -> String {
    let title  = m.window_title.as_deref().unwrap_or(&m.name);
    let width  = m.window_width.unwrap_or(1024);
    let height = m.window_height.unwrap_or(768);
    let front  = m.frontend.as_ref().map(|f| f.display().to_string())
    .unwrap_or_else(|| "../frontend".into());
    format!(
        r#"{{"build":{{"devPath":"{f}","distDir":"{f}"}},"package":{{"productName":"{t}","version":"{v}"}},"tauri":{{"allowlist":{{"all":true}},"bundle":{{"active":true,"identifier":"pl.hackeros.{n}","targets":"all"}},"security":{{"csp":null}},"windows":[{{"height":{h},"resizable":true,"title":"{t}","width":{w}}}]}}}}"#,
        f=front, t=title, v=m.version, n=m.name, h=height, w=width
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
    let out_dir = base.join(".vira-out");
    cmd_build(path, false, false, false, false, Some(&out_dir), verbose)?;
    let manifest = ViraManifest::load(path)?;
    let binary = out_dir.join(format!("target/debug/{}", manifest.name));
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
