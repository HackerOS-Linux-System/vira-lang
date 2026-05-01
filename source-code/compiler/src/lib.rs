pub mod arena;
pub mod build_integration;
pub mod cargo_gen;
pub mod codegen;
pub mod native_api;

use std::path::{Path, PathBuf};
use anyhow::{Context, Result};

use vira_parser::parse;
use codegen::CodegenContext;
use cargo_gen::CargoGen;
use build_integration::BuildIntegration;
use native_api::{NativeApiKind};

/// Full compilation result for one Vira source file.
pub struct CompileOutput {
    /// Generated Rust source code
    pub rust_source: String,
    /// Generated Cargo.toml
    pub cargo_toml: String,
    /// Optional build.rs (Tauri projects)
    pub build_rs: Option<String>,
    /// Optional CMakeLists.txt
    pub cmake: Option<String>,
    /// Optional Makefile
    pub makefile: Option<String>,
}

/// Compile a Vira source string, given a project name.
pub fn compile(
    source: &str,
    project_name: &str,
    emit_cmake: bool,
    emit_makefile: bool,
) -> Result<CompileOutput> {
    // 1. Parse
    let program = parse(source)
        .map_err(|e| anyhow::anyhow!("Parse error: {e}"))?;

    // 2. Code generation
    let mut ctx = CodegenContext::new();
    let rust_source = ctx.generate(&program);
    let native_apis = ctx.native_apis.clone();

    // 3. Cargo.toml
    let cargo_gen = CargoGen::new(project_name, native_apis.clone());
    let cargo_toml = cargo_gen.generate();
    let build_rs = cargo_gen.generate_build_rs();

    // 4. Build system integration
    let has_tauri = native_apis.iter().any(|a| a.kind == NativeApiKind::Tauri);
    let has_gtk   = native_apis.iter().any(|a| a.kind == NativeApiKind::Gtk);
    let has_qt    = native_apis.iter().any(|a| a.kind == NativeApiKind::Qt);
    let build_int = BuildIntegration::new(project_name, has_tauri, has_gtk, has_qt);

    let cmake    = emit_cmake.then(|| build_int.cmake());
    let makefile = emit_makefile.then(|| build_int.makefile());

    Ok(CompileOutput {
        rust_source,
        cargo_toml,
        build_rs,
        cmake,
        makefile,
    })
}

/// Write CompileOutput to an output directory.
pub fn write_output(output: &CompileOutput, out_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(out_dir.join("src"))
        .with_context(|| format!("creating output dir {}", out_dir.display()))?;

    std::fs::write(out_dir.join("src/main.rs"), &output.rust_source)
        .context("writing src/main.rs")?;
    std::fs::write(out_dir.join("Cargo.toml"), &output.cargo_toml)
        .context("writing Cargo.toml")?;

    if let Some(build_rs) = &output.build_rs {
        std::fs::write(out_dir.join("build.rs"), build_rs)
            .context("writing build.rs")?;
    }
    if let Some(cmake) = &output.cmake {
        std::fs::write(out_dir.join("CMakeLists.txt"), cmake)
            .context("writing CMakeLists.txt")?;
    }
    if let Some(makefile) = &output.makefile {
        std::fs::write(out_dir.join("Makefile"), makefile)
            .context("writing Makefile")?;
    }

    Ok(())
}
