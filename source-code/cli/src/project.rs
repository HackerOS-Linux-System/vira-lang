use std::path::Path;
use anyhow::Result;
use colored::Colorize;
use crate::Template;

pub fn create(name: &str, template: Template, _verbose: bool) -> Result<()> {
    let dir = Path::new(name);
    if dir.exists() {
        anyhow::bail!("directory '{}' already exists", name);
    }

    std::fs::create_dir_all(dir.join("src"))?;
    std::fs::create_dir_all(dir.join("lib"))?;

    // icons/ only for GUI apps
    let needs_icons = matches!(template, Template::Tauri | Template::Gtk | Template::Qt);
    if needs_icons {
        std::fs::create_dir_all(dir.join("icons"))?;
        std::fs::write(dir.join("icons/.gitkeep"), b"")?;
    }

    std::fs::write(dir.join("vira.hk"),       vira_hk(name, &template))?;
    std::fs::write(dir.join("manifest.toml"), manifest_toml(name, &template))?;
    std::fs::write(dir.join("src/main.vira"), main_vira(name, &template))?;
    std::fs::write(dir.join("lib/index.html"),lib_html(name, &template))?;
    std::fs::write(dir.join(".gitignore"),    GITIGNORE)?;
    std::fs::write(dir.join("README.md"),     readme(name, &template))?;

    if matches!(template, Template::Qt) {
        std::fs::create_dir_all(dir.join("lib/qml"))?;
        std::fs::write(dir.join("lib/qml/main.qml"), QML_MAIN)?;
    }

    println!("  {} Created project structure:", "✓".green());
    println!("     {name}/");
    println!("     ├── src/main.vira       ← backend logic");
    println!("     ├── lib/index.html      ← UI (edit this)");
    if needs_icons { println!("     ├── icons/              ← app icons (add 32x32.png etc.)"); }
    println!("     ├── vira.hk             ← project config (HK format)");
    println!("     └── manifest.toml       ← app manifest");

    Ok(())
}

fn vira_hk(name: &str, t: &Template) -> String {
    let target = match t {
        Template::Tauri => "tauri",
        Template::Gtk   => "gtk",
        Template::Qt    => "qt",
        Template::Cli   => "cli",
        Template::Lib   => "lib",
    };
    let window_section = if matches!(t, Template::Tauri) {
        format!(
            "\n[tauri]\n-> window_title  => {name}\n-> window_width  => 1024\n-> window_height => 768\n-> frontend      => lib/index.html\n"
        )
    } else {
        String::new()
    };
    // NOTE: "Your Name" can't be inside format!() — Rust 2021 treats Name as identifier prefix
    let author = String::from("[\"Your Name Here\"]");
    let mut out = String::new();
    out.push_str("! vira.hk\n");
    out.push_str("! Format: HackerOS .hk\n\n");
    out.push_str("[package]\n");
    out.push_str(&format!("-> name        => {name}\n"));
    out.push_str("-> version     => 0.1.0\n");
    out.push_str(&format!("-> description => {name} written in Vira\n"));
    out.push_str(&format!("-> authors     => {author}\n\n"));
    out.push_str("[build]\n");
    out.push_str("-> entry  => src/main.vira\n");
    out.push_str(&format!("-> target => {target}\n"));
    out.push_str(&window_section);
    out
}


// Legacy - kept for compatibility
fn vira_toml(name: &str, t: &Template) -> String {
    vira_hk(name, t)
}

fn manifest_toml(name: &str, t: &Template) -> String {
    let is_gui = matches!(t, Template::Tauri | Template::Gtk | Template::Qt);
    if !is_gui {
        return format!("[app]\nname = \"{name}\"\nversion = \"0.1.0\"\n");
    }
    format!(
        r#"[app]
        name        = "{name}"
        version     = "0.1.0"
        identifier  = "pl.hackeros.{name}"
        description = "{name} app"

        [window]
        title     = "{name}"
        width     = 1024
        height    = 768
        resizable = true

        [bundle]
        # true = require icons in icons/  |  false = auto-generate placeholder icons
        icons   = false
        targets = "all"

        [build]
        # UI directory: write your interface here in .vira or .html
        ui_dir = "lib"
        "#)
}

fn main_vira(name: &str, t: &Template) -> String {
    match t {
        Template::Tauri => format!(
            r#";; src/main.vira — {name}
            ;; Backend logic. UI lives in lib/

            use <tauri>

            /// Example command — call from lib/ with invoke('greet', {{name: 'world'}})
            pub async fn greet(name: str) -> str {{
            "Hello, " + name + "! Built with Vira + Tauri."
    }}

    pub fn main() -> void! {{
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())?
    }}
    "#),
    Template::Gtk => format!(
        r#";; src/main.vira — {name}
        use <gtk>

        pub fn main() -> void! {{
        let app = gtk::Application::new("pl.hackeros.{name}", gtk::ApplicationFlags::default())
    app.connect_activate(|app| {{
    let win = gtk::ApplicationWindow::new(app)
    win.set_title("{name}")
    win.set_default_size(1024, 768)
    let label = gtk::Label::new("Hello from Vira + GTK4!")
    win.set_child(label)
    win.present()
    }})
    app.run()
    }}
    "#),
    Template::Qt => format!(
        r#";; src/main.vira — {name}
        use <qt>

        pub fn main() -> void! {{
        let app = qt::QGuiApplication::new()
    let engine = qt::QQmlApplicationEngine::new()
    engine.load("qrc:/qml/main.qml")
    app.exec()?
    }}
    "#),
    Template::Cli => format!(
        r#";; src/main.vira — {name}

        pub fn main() -> void! {{
        println("Hello from {name}!")
    }}

    extern {{
    fn println(s: str) -> void
    }}
    "#),
    Template::Lib => format!(
        r#";; src/lib.vira — {name}

        /// Add two numbers
        pub fn add(a: i32, b: i32) -> i32 {{
        a + b
    }}
    "#),
    }
}

fn lib_html(name: &str, t: &Template) -> String {
    match t {
        Template::Tauri => format!(
            r#"<!-- lib/index.html — UI for {name} -->
            <!-- This is your app's interface. Call Vira backend with invoke(). -->
            <!DOCTYPE html>
            <html lang="en">
            <head>
            <meta charset="UTF-8" />
            <title>{name}</title>
            <style>
            body {{ font-family: monospace; background: #0d0d0d; color: #e8e8e8; display: flex; align-items: center; justify-content: center; height: 100vh; margin: 0; }}
            button {{ background: #00ff88; color: #000; border: none; padding: 10px 24px; font-family: monospace; font-size: 14px; cursor: pointer; border-radius: 6px; }}
            h1 {{ color: #00ff88; }}
            </style>
            </head>
            <body>
            <div>
            <h1>{name}</h1>
            <p>Built with <strong>Vira</strong> → Tauri</p>
            <button onclick="callBackend()">Call Backend</button>
            <p id="result"></p>
            </div>
            <script>
            const {{ invoke }} = window.__TAURI__.tauri;
            async function callBackend() {{
            const result = await invoke('greet', {{ name: 'World' }});
        document.getElementById('result').textContent = result;
    }}
    </script>
    </body>
    </html>
    "#),
    _ => format!(
        r#"<!-- lib/index.html — UI for {name} -->
        <!DOCTYPE html>
        <html><head><meta charset="UTF-8"><title>{name}</title></head>
        <body><h1>{name}</h1><p>Written in Vira</p></body>
        </html>
        "#),
    }
}

fn readme(name: &str, t: &Template) -> String {
    let target = match t {
        Template::Tauri => "Tauri", Template::Gtk => "GTK4",
        Template::Qt    => "Qt6",   Template::Cli => "CLI",
        Template::Lib   => "lib",
    };
    format!(
        r#"# {name}

        **{target}** app written in **Vira**.

        ## Project structure

        ```
        {name}/
        ├── src/main.vira     — backend logic (Vira → Rust)
        ├── lib/index.html    — UI (edit this)
        ├── icons/            — app icons
        ├── vira.toml         — project config
        └── manifest.toml     — app manifest (window size, icons, bundle)
        ```

        ## Build

        ```sh
        vira build                # debug
        vira build --production   # release + installer bundle
        vira run                  # build & run
        ```

        ## Syntax

        ```vira
        ;; comment
        use <tauri>             ;; native Tauri API
        using <serde> from <crates>     ;; Rust crate
        using <react> from <npm>        ;; npm package (UI layer)
        usage <my-lib>          ;; Vira registry (coming soon)
        ```
        "#)
}

const GITIGNORE: &str = "build/\ntarget/\n*.rs.bak\n.DS_Store\n";

const QML_MAIN: &str = r#"import QtQuick 2.15
import QtQuick.Controls 2.15
ApplicationWindow {
visible: true; width: 1024; height: 768; title: "Vira Qt App"
Label { anchors.centerIn: parent; text: "Hello from Vira + Qt6!"; font.pixelSize: 24 }
}
"#;
