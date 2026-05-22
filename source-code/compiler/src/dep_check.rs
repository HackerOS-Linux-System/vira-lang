#[derive(Debug, Clone)]
pub struct MissingDep {
    pub name: &'static str,
    pub description: &'static str,
    pub apt_package: &'static str,
    pub cargo_install: Option<&'static str>,
}

/// Known system dependencies and how to install them on Debian/Ubuntu
static KNOWN_DEPS: &[MissingDep] = &[
    MissingDep {
        name: "pkg-config",
        description: "Build configuration tool",
        apt_package: "pkg-config",
        cargo_install: None,
    },
MissingDep {
    name: "libgtk-4-dev",
    description: "GTK4 development headers",
    apt_package: "libgtk-4-dev",
    cargo_install: None,
},
MissingDep {
    name: "libglib2.0-dev",
    description: "GLib development headers",
    apt_package: "libglib2.0-dev",
    cargo_install: None,
},
MissingDep {
    name: "libjavascriptcoregtk-4.1",
    description: "WebKit JavaScriptCore (required by Tauri)",
    apt_package: "libjavascriptcoregtk-4.1-dev",
    cargo_install: None,
},
MissingDep {
    name: "libwebkit2gtk",
    description: "WebKit2GTK (required by Tauri)",
    apt_package: "libwebkit2gtk-4.1-dev",
    cargo_install: None,
},
MissingDep {
    name: "libayatana-appindicator3",
    description: "AppIndicator (for Tauri system tray)",
    apt_package: "libayatana-appindicator3-dev",
    cargo_install: None,
},
MissingDep {
    name: "librsvg",
    description: "SVG rendering library",
    apt_package: "librsvg2-dev",
    cargo_install: None,
},
MissingDep {
    name: "tauri-cli",
    description: "Tauri command-line tools",
    apt_package: "cargo",
    cargo_install: Some("tauri-cli"),
},
MissingDep {
    name: "libssl-dev",
    description: "OpenSSL development headers",
    apt_package: "libssl-dev",
    cargo_install: None,
},
MissingDep {
    name: "build-essential",
    description: "C/C++ build tools",
    apt_package: "build-essential",
    cargo_install: None,
},
MissingDep {
    name: "xdg-utils",
    description: "XDG utilities (for Tauri)",
    apt_package: "xdg-utils",
    cargo_install: None,
},
];

/// Parse cargo/rustc output and convert to Vira-style friendly messages
pub fn translate_cargo_error(cargo_output: &str) -> String {
    let mut messages = Vec::new();
    let mut apt_packages: Vec<String> = Vec::new();
    let mut cargo_installs: Vec<String> = Vec::new();

    for line in cargo_output.lines() {
        // Check for known missing system libraries
        for dep in KNOWN_DEPS {
            if line.contains(dep.apt_package)
                || line.contains(dep.name)
                || (dep.name.starts_with("lib") && line.contains(&dep.name[3..]))
                {
                    let msg = format!(
                        "  {} Brakująca zależność: {} — {}",
                        "✗",
                        dep.name,
                        dep.description
                    );
                    if !messages.contains(&msg) {
                        messages.push(msg);
                        if !apt_packages.contains(&dep.apt_package.to_string()) {
                            apt_packages.push(dep.apt_package.to_string());
                        }
                        if let Some(ci) = dep.cargo_install {
                            if !cargo_installs.contains(&ci.to_string()) {
                                cargo_installs.push(ci.to_string());
                            }
                        }
                    }
                }
        }

        // Generic "not found" or "could not find"
        if line.contains("could not find") && line.contains("pkg-config") {
            messages.push(format!(
                "  {} pkg-config nie jest zainstalowany",
                "✗"
            ));
            if !apt_packages.contains(&"pkg-config".to_string()) {
                apt_packages.push("pkg-config".to_string());
            }
        }

        // protocol-asset feature error
        if line.contains("protocol-asset") {
            messages.push(format!(
                "  {} Błąd konfiguracji Tauri: nieznana cecha 'protocol-asset'",
                "✗"
            ));
            messages.push("     Vira automatycznie usunęła tę cechę — spróbuj ponownie.".into());
        }

        // tauri.conf.json relative URL error
        if line.contains("relative URL without a base") {
            messages.push(format!(
                "  {} Błąd konfiguracji Tauri: nieprawidłowa ścieżka frontendDist",
                "✗"
            ));
            messages.push("     Vira poprawiła tę ścieżkę — spróbuj ponownie.".into());
        }
    }

    // If no specific deps found but cargo failed, show generic message
    if messages.is_empty() && cargo_output.contains("error") {
        messages.push(format!(
            "  {} Błąd kompilacji Rust (szczegóły poniżej)",
                              "✗"
        ));
    }

    let mut out = String::new();

    if !messages.is_empty() {
        out.push_str(&format!("\n{}", "── Błędy zależności ────────────────────"));
        for msg in &messages {
            out.push_str(&format!("\n{msg}"));
        }
        out.push('\n');
    }

    // Show install commands
    if !apt_packages.is_empty() {
        out.push_str(&format!("\n  {} Zainstaluj brakujące pakiety:", "Naprawienie:"));
        out.push_str(&format!("\n  {} sudo apt install -y {}\n",
                              "›",
                              apt_packages.join(" ")
        ));
    }
    if !cargo_installs.is_empty() {
        for ci in &cargo_installs {
            out.push_str(&format!("  {} cargo install {}\n", "›", ci));
        }
    }

    out
}

/// Full Tauri dependency list for fresh Debian install
pub fn print_tauri_deps() {
    println!("\n  {} Wymagane zależności dla Tauri v2 na Debian/Ubuntu:", "Tauri");
    println!();
    println!("  {} sudo apt install -y \\", "›");
    println!("      build-essential \\");
    println!("      pkg-config \\");
    println!("      libssl-dev \\");
    println!("      libgtk-3-dev \\");
    println!("      libwebkit2gtk-4.1-dev \\");
    println!("      libjavascriptcoregtk-4.1-dev \\");
    println!("      libayatana-appindicator3-dev \\");
    println!("      librsvg2-dev \\");
    println!("      xdg-utils");
    println!();
    println!("  {} cargo install tauri-cli", "›");
    println!();
}

/// Check if a required tool is available
pub fn check_tool(name: &str) -> bool {
    std::process::Command::new(name)
    .arg("--version")
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .status()
    .map(|s| s.success())
    .unwrap_or(false)
}

/// Check all Vira build prerequisites and report missing ones
pub fn check_prerequisites(target: &str) -> Vec<String> {
    let mut missing = Vec::new();

    // Always need cargo/rust
    if !check_tool("cargo") {
        missing.push("Rust/Cargo nie zainstalowany: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh".into());
    }

    match target {
        "tauri" => {
            // Check system libs via pkg-config
            for lib in &["webkit2gtk-4.1", "gtk+-3.0", "ayatana-appindicator3-0.1"] {
                let ok = std::process::Command::new("pkg-config")
                .args(["--exists", lib])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
                if !ok {
                    let apt = match *lib {
                        "webkit2gtk-4.1" => "libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev",
                        "gtk+-3.0"       => "libgtk-3-dev",
                        _                => "libayatana-appindicator3-dev",
                    };
                    missing.push(format!("sudo apt install -y {apt}"));
                }
            }
        }
        "gtk" => {
            let ok = std::process::Command::new("pkg-config")
            .args(["--exists", "gtk4"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
            if !ok {
                missing.push("sudo apt install -y libgtk-4-dev".into());
            }
        }
        _ => {}
    }

    missing
}
