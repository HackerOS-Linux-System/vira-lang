pub mod arena;
pub mod build_cache;
pub mod build_integration;
pub mod cargo_gen;
pub mod codegen;
pub mod diagnostics;
pub mod native_api;

use std::path::{Path, PathBuf};
use anyhow::{Context, Result};

use vira_parser::parse;
use codegen::CodegenContext;
use cargo_gen::CargoGen;
use build_integration::BuildIntegration;
use build_cache::{BuildCache, CacheResult, check_cache};
use diagnostics::{DiagnosticBag, parse_error_to_diagnostic};
use native_api::NativeApiKind;

pub use diagnostics::{Diagnostic, DiagnosticBag as Diagnostics, Severity};
pub use build_cache::{build_root, cache_root, project_out_dir};

pub struct CompileOutput {
    pub rust_source: String,
    pub cargo_toml: String,
    pub build_rs: Option<String>,
    pub cmake: Option<String>,
    pub makefile: Option<String>,
}

pub fn compile(
    source: &str,
    project_name: &str,
    emit_cmake: bool,
    emit_makefile: bool,
) -> Result<CompileOutput> {
    let program = parse(source).map_err(|e| {
        let mut bag = DiagnosticBag::new();
        bag.push(parse_error_to_diagnostic(&e, &format!("{project_name}.vira")));
        anyhow::anyhow!("{}", bag.render_all(Some(source)))
    })?;

    let mut ctx = CodegenContext::new();
    let rust_source = ctx.generate(&program);
    let native_apis = ctx.native_apis.clone();

    let cargo_gen = CargoGen::new(project_name, native_apis.clone());
    let cargo_toml = cargo_gen.generate();
    let build_rs = cargo_gen.generate_build_rs();

    let has_tauri = native_apis.iter().any(|a| a.kind == NativeApiKind::Tauri);
    let has_gtk   = native_apis.iter().any(|a| a.kind == NativeApiKind::Gtk);
    let has_qt    = native_apis.iter().any(|a| a.kind == NativeApiKind::Qt);
    let build_int = BuildIntegration::new(project_name, has_tauri, has_gtk, has_qt);

    Ok(CompileOutput {
        rust_source,
       cargo_toml,
       build_rs,
       cmake: emit_cmake.then(|| build_int.cmake()),
       makefile: emit_makefile.then(|| build_int.makefile()),
    })
}

pub fn build_project(
    files: &[PathBuf],
    project_name: &str,
    release: bool,
    emit_cmake: bool,
    emit_makefile: bool,
) -> Result<PathBuf> {
    let (cache_result, cache) = check_cache(project_name, files, release)
    .context("cache check")?;

    let (rust_source, cargo_toml) = match cache_result {
        CacheResult::Hit { rust_source, cargo_toml } => (rust_source, cargo_toml),
        CacheResult::Miss => {
            let mut combined = String::new();
            for f in files {
                combined.push_str(&std::fs::read_to_string(f)
                .with_context(|| format!("reading {}", f.display()))?);
                combined.push('\n');
            }
            let out = compile(&combined, project_name, emit_cmake, emit_makefile)?;
            let sources = cache.scan_sources(files)?;
            let hash = BuildCache::combined_hash(&sources);
            cache.store_rust_source(&hash, &out.rust_source)?;
            cache.store_cargo_toml(&out.cargo_toml)?;
            cache.update_manifest(sources, &hash, vec![])?;
            (out.rust_source, out.cargo_toml)
        }
    };

    let rust_out = cache.prepare_out_dir()?;
    std::fs::write(rust_out.join("src/main.rs"), &rust_source)?;
    std::fs::write(rust_out.join("Cargo.toml"), &cargo_toml)?;

    let target_dir = cache.cargo_target_dir();
    let mut cmd = std::process::Command::new("cargo");
    cmd.arg("build")
    .arg("--manifest-path").arg(rust_out.join("Cargo.toml"))
    .env("CARGO_TARGET_DIR", &target_dir);
    if release { cmd.arg("--release"); }
    let status = cmd.status().context("cargo build")?;
    if !status.success() { anyhow::bail!("cargo build failed"); }

    let profile_dir = if release { "release" } else { "debug" };
    let binary_src = target_dir.join(profile_dir).join(project_name);
    let binary_dest = cache.write_binary(&binary_src, release)?;
    cache.evict_old_entries(5).ok();
    Ok(binary_dest)
}

pub fn write_output(output: &CompileOutput, out_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(out_dir.join("src"))?;
    std::fs::write(out_dir.join("src/main.rs"), &output.rust_source)?;
    std::fs::write(out_dir.join("Cargo.toml"), &output.cargo_toml)?;
    if let Some(b) = &output.build_rs  { std::fs::write(out_dir.join("build.rs"), b)?; }
    if let Some(c) = &output.cmake     { std::fs::write(out_dir.join("CMakeLists.txt"), c)?; }
    if let Some(m) = &output.makefile  { std::fs::write(out_dir.join("Makefile"), m)?; }
    Ok(())
}
