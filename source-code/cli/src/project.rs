use std::path::Path;
use anyhow::{Context, Result};
use colored::Colorize;

use crate::Template;

pub fn create(name: &str, template: Template, verbose: bool) -> Result<()> {
    let dir = Path::new(name);
    if dir.exists() {
        anyhow::bail!("directory '{}' already exists", name);
    }

    std::fs::create_dir_all(dir.join("src"))?;

    // vira.toml
    std::fs::write(dir.join("vira.toml"), vira_toml(name, &template))
    .context("writing vira.toml")?;

    // main source
    std::fs::write(dir.join("src/main.vira"), main_vira(name, &template))
    .context("writing src/main.vira")?;

    // .gitignore
    std::fs::write(dir.join(".gitignore"), GITIGNORE)
    .context("writing .gitignore")?;

    // README
    std::fs::write(dir.join("README.md"), readme(name, &template))
    .context("writing README.md")?;

    // Tauri gets a minimal index.html for the frontend
    if matches!(template, Template::Tauri) {
        std::fs::create_dir_all(dir.join("frontend"))?;
        std::fs::write(dir.join("frontend/index.html"), TAURI_INDEX_HTML)
        .context("writing frontend/index.html")?;
    }

    // Qt gets a minimal QML file
    if matches!(template, Template::Qt) {
        std::fs::create_dir_all(dir.join("qml"))?;
        std::fs::write(dir.join("qml/main.qml"), QT_MAIN_QML)
        .context("writing qml/main.qml")?;
    }

    println!("\n{} {} ({})\n", "Created".green().bold(), name.cyan().bold(), template_name(&template));
    println!("  {} cd {}", "→".dimmed(), name);
    println!("  {} vira build", "→".dimmed());
    println!("  {} vira run\n", "→".dimmed());

    Ok(())
}

fn template_name(t: &Template) -> &'static str {
    match t {
        Template::Tauri => "Tauri app",
        Template::Gtk   => "GTK4 app",
        Template::Qt    => "Qt6 app",
        Template::Cli   => "CLI app",
        Template::Lib   => "library",
    }
}

fn vira_toml(name: &str, t: &Template) -> String {
    let target = match t {
        Template::Tauri => "tauri",
        Template::Gtk   => "gtk",
        Template::Qt    => "qt",
        Template::Cli   => "cli",
        Template::Lib   => "lib",
    };
    format!(
        r#"[package]
        name = "{name}"
        version = "0.1.0"
        description = "{name} — written in Vira"
        authors = ["Your Name"]

        [build]
        entry = "src/main.vira"
        target = "{target}"
        "#
    )
}

fn main_vira(name: &str, t: &Template) -> String {
    match t {
        Template::Tauri => format!(
            r#"// src/main.vira — {name} (Tauri)
        // Vira — pisz wszystko w jednym języku, bez JS/TS + Rust split

        use <tauri>

        /// Przywitaj użytkownika — przykładowy Tauri command
        pub async fn greet(name: str) -> str {{
        "Cześć, " + name + "! Witaj w " + "{name}" + " napisanym w Vira."
    }}

    pub fn main() -> void! {{
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())?
    }}
    "#,
    name = name
        ),
        Template::Gtk => format!(
            r#"// src/main.vira — {name} (GTK4)

        use <gtk:4.0>

        pub fn main() -> void! {{
        let app = gtk::Application::new("pl.hackeros.{name}", gtk::ApplicationFlags::default())

        app.connect_activate(|app| {{
        let win = gtk::ApplicationWindow::new(app)
        win.set_title("{name}")
        win.set_default_size(800, 600)

        let label = gtk::Label::new("Cześć z Vira + GTK4!")
        win.set_child(label)
        win.present()
    }})

        app.run()
    }}
    "#,
    name = name
        ),
        Template::Qt => format!(
            r#"// src/main.vira — {name} (Qt6)

        use <qt:6>

        pub fn main() -> void! {{
        let app = qt::QGuiApplication::new()
        let engine = qt::QQmlApplicationEngine::new()
        engine.load("qrc:/qml/main.qml")
        app.exec()?
    }}
    "#,
    name = name
        ),
        Template::Cli => format!(
            r#"// src/main.vira — {name} (CLI)

        /// Punkt wejścia programu
        pub fn main() -> void! {{
        let args = env_args()
        println("Cześć z {name}!")
        println("Argumenty: " + args.len().to_string())
    }}

    extern {{
    fn println(s: str) -> void
    fn env_args() -> [str]
    }}
    "#,
    name = name
        ),
        Template::Lib => format!(
            r#"// src/main.vira — {name} (library)

        /// Przykładowa funkcja biblioteki
        pub fn add(a: i32, b: i32) -> i32 {{
        a + b
    }}

    pub fn version() -> str {{
    "0.1.0"
    }}
    "#,
    name = name
        ),
    }
}

fn readme(name: &str, t: &Template) -> String {
    format!(
        "# {name}\n\n{} project written in **Vira**.\n\n## Build\n\n```sh\nvira build\nvira run\n```\n",
        template_name(t)
    )
}

const GITIGNORE: &str = ".vira-out/\n/build/\ntarget/\n*.rs.bak\n";

const TAURI_INDEX_HTML: &str = r#"<!DOCTYPE html>
<html lang="pl">
<head>
<meta charset="UTF-8">
<title>Vira Tauri App</title>
</head>
<body>
<h1>Witaj w Vira + Tauri!</h1>
<button onclick="greet()">Przywitaj</button>
<script>
const { invoke } = window.__TAURI__.tauri;
async function greet() {
const msg = await invoke('greet', { name: 'HackerOS' });
alert(msg);
}
</script>
</body>
</html>
"#;

const QT_MAIN_QML: &str = r#"import QtQuick 2.15
import QtQuick.Controls 2.15

ApplicationWindow {
visible: true
width: 800
height: 600
title: "Vira Qt App"

Label {
anchors.centerIn: parent
text: "Witaj z Vira + Qt6!"
font.pixelSize: 24
}
}
"#;
