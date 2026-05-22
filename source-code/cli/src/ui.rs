use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
use std::time::Duration;

// ─── Colors ───────────────────────────────────────────────────────────────────

pub const GREEN:  &str = "\x1b[32m";
pub const CYAN:   &str = "\x1b[36m";
pub const YELLOW: &str = "\x1b[33m";
pub const DIM:    &str = "\x1b[2m";
pub const BOLD:   &str = "\x1b[1m";
pub const RESET:  &str = "\x1b[0m";

// ─── Transpile spinner (yarn-style bouncing dots) ─────────────────────────────

pub fn transpile_spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template(
            "  {spinner:.cyan} {msg}"
        )
        .unwrap()
        .tick_strings(&[
            "⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷",
        ]),
    );
    pb.set_message(msg.to_owned());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}

pub fn download_spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template(
            "  {spinner:.green} {msg} {elapsed:.dim}"
        )
        .unwrap()
        .tick_strings(&[
            "◐", "◓", "◑", "◒",
        ]),
    );
    pb.set_message(msg.to_owned());
    pb.enable_steady_tick(Duration::from_millis(100));
    pb
}

// ─── Build progress bar (single, smooth fill) ─────────────────────────────────

pub fn build_bar(total_steps: u64, name: &str) -> ProgressBar {
    let pb = ProgressBar::new(total_steps);
    pb.set_style(
        ProgressStyle::with_template(
            "  {bar:40.cyan/238} {pos:>2}/{len:2}  {msg:.dim}"
        )
        .unwrap()
        .progress_chars("█▉▊▋▌▍▎▏  "),
    );
    pb.set_message(format!("building {name}"));
    pb
}

// ─── Cargo wrapper — captures cargo output and shows a progress bar ───────────

pub struct CargoBuildProgress {
    pub multi: MultiProgress,
    pub main_bar: ProgressBar,
    pub status_bar: ProgressBar,
}

impl CargoBuildProgress {
    pub fn new(project_name: &str) -> Self {
        let multi = MultiProgress::new();

        let main_bar = multi.add(ProgressBar::new(100));
        main_bar.set_style(
            ProgressStyle::with_template(
                "  {bar:42.cyan/238} {percent:>3}%  {elapsed_precise:.dim}"
            )
            .unwrap()
            .progress_chars("█▉▊▋▌▍▎▏  "),
        );
        main_bar.set_message(format!("compiling {project_name}"));

        let status_bar = multi.add(ProgressBar::new_spinner());
        status_bar.set_style(
            ProgressStyle::with_template(
                "  {spinner:.cyan.dim} {msg:.dim}"
            )
            .unwrap()
            .tick_strings(&["⣾","⣽","⣻","⢿","⡿","⣟","⣯","⣷"]),
        );
        status_bar.enable_steady_tick(Duration::from_millis(80));

        CargoBuildProgress { multi, main_bar, status_bar }
    }

    pub fn set_status(&self, msg: &str) {
        self.status_bar.set_message(msg.to_owned());
    }

    pub fn set_progress(&self, pct: u64) {
        self.main_bar.set_position(pct.min(99));
    }

    pub fn finish_ok(&self, msg: &str) {
        self.main_bar.set_position(100);
        self.main_bar.set_style(
            ProgressStyle::with_template(
                "  {bar:42.green/238} {percent:>3}%  {elapsed_precise:.dim}"
            )
            .unwrap()
            .progress_chars("█▉▊▋▌▍▎▏  "),
        );
        self.status_bar.finish_and_clear();
        self.main_bar.finish_with_message(msg.to_owned());
    }

    pub fn finish_err(&self, msg: &str) {
        self.main_bar.set_style(
            ProgressStyle::with_template(
                "  {bar:42.red/238} {percent:>3}%  {elapsed_precise:.dim}"
            )
            .unwrap()
            .progress_chars("█▉▊▋▌▍▎▏  "),
        );
        self.status_bar.finish_and_clear();
        self.main_bar.abandon_with_message(msg.to_owned());
    }
}

// ─── Run cargo with progress bar ──────────────────────────────────────────────
// Cargo outputs JSON build plan when --message-format=json is used.
// We parse that to drive the progress bar.

pub fn run_cargo_with_progress(
    args: &[&str],
    manifest_path: &std::path::Path,
    project_name: &str,
) -> anyhow::Result<()> {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    use anyhow::Context;

    let prog = CargoBuildProgress::new(project_name);
    prog.set_status("resolving dependencies...");
    prog.set_progress(2);

    let mut cmd = Command::new("cargo");
    cmd.args(args)
    .arg("--manifest-path").arg(manifest_path)
    .arg("--message-format=json")
    .stdout(Stdio::piped())
    .stderr(Stdio::piped()); // hide raw cargo stderr

    let mut child = cmd.spawn()
    .context("cargo not found — install Rust from https://rustup.rs")?;
    let child_stderr = child.stderr.take();

    let stdout = child.stdout.take().unwrap();
    let reader = BufReader::new(stdout);

    let mut compiled = 0u64;
    let mut total    = 0u64;
    let mut last_pct = 2u64;

    for line in reader.lines() {
        let line = match line { Ok(l) => l, Err(_) => continue };
        // Parse cargo JSON messages
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
            let reason = json.get("reason").and_then(|v| v.as_str()).unwrap_or("");
            match reason {
                "build-script-executed" => {
                    let pkg = json.get("package_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.split(' ').next())
                    .unwrap_or("...");
                    prog.set_status(&format!("build script: {pkg}"));
                    last_pct = (last_pct + 3).min(40);
                    prog.set_progress(last_pct);
                }
                "compiler-artifact" => {
                    compiled += 1;
                    if total == 0 { total = compiled + 5; }
                    let pkg = json.get("package_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.split(' ').next())
                    .unwrap_or("crate");
                    prog.set_status(&format!("compiling {pkg}"));
                    let pct = 40 + (compiled * 55 / total.max(1));
                    prog.set_progress(pct.min(95));
                }
                "compiler-message" => {
                    // warnings etc — silently ignore for clean UI
                }
                "build-finished" => {
                    let success = json.get("success")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                    if success {
                        prog.finish_ok("done");
                    } else {
                        prog.finish_err("failed");
                    }
                }
                _ => {}
            }
        }
    }

    // Capture stderr for friendly dep error translation    let mut cargo_stderr = String::new();    if let Some(mut se) = child_stderr {        use std::io::Read;        se.read_to_string(&mut cargo_stderr).ok();    }    let status = child.wait().context("waiting for cargo")?;    if !status.success() {        // Show Vira-style dependency errors first        let friendly = vira_compiler::dep_check::translate_cargo_error(&cargo_stderr);        if !friendly.is_empty() {            eprint!("{friendly}");        }        // Re-run to show raw errors        eprintln!();        let _ = Command::new("cargo")            .args(args)            .arg("--manifest-path").arg(manifest_path)            .status();        anyhow::bail!("cargo build failed");    }
    Ok(())
}

// ─── Tauri build with progress ────────────────────────────────────────────────

pub fn run_tauri_with_progress(
    out_dir: &std::path::Path,
    release: bool,
    project_name: &str,
) -> anyhow::Result<()> {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};
    use anyhow::Context;

    let prog = CargoBuildProgress::new(project_name);
    prog.set_status("starting tauri...");
    prog.set_progress(5);

    let args = vec!["tauri", "build"];
    if !release { } // tauri dev not captured this way

    let mut cmd = Command::new("cargo");
    cmd.args(&args)
    .current_dir(out_dir)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

    let mut child = cmd.spawn()
    .context("cargo tauri not found — run: cargo install tauri-cli")?;

    let stderr = child.stderr.take().unwrap();
    let reader = BufReader::new(stderr);

    let phases = [
        ("Compiling",  20u64), ("Bundling",  60u64),
        ("Finished",   90u64), ("Built",    100u64),
    ];

    for line in reader.lines() {
        let line = match line { Ok(l) => l, Err(_) => continue };
        let trimmed = line.trim();
        for (keyword, pct) in &phases {
            if trimmed.contains(keyword) {
                prog.set_status(&format!("{trimmed:.60}"));
                prog.set_progress(*pct);
            }
        }
        // Tauri compiling lines
        if trimmed.starts_with("Compiling") || trimmed.starts_with("Finished") {
            let short: String = trimmed.chars().take(70).collect();
            prog.set_status(&short);
        }
    }

    let status = child.wait().context("tauri build")?;
    if status.success() {
        prog.finish_ok("bundle ready");
        Ok(())
    } else {
        prog.finish_err("tauri build failed");
        anyhow::bail!("tauri build failed — run `cargo tauri build` manually for details")
    }
}
