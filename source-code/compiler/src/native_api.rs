use std::collections::HashMap;

/// Resolved native API dependency.
#[derive(Debug, Clone)]
pub struct NativeApi {
    pub name: String,
    pub version: Option<String>,
    pub kind: NativeApiKind,
    /// Rust crate names to add to generated Cargo.toml
    pub crates: Vec<CrateDep>,
    /// Rust `use` statements to prepend to generated code
    pub rust_prelude: Vec<String>,
    /// Feature flags required
    pub features: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NativeApiKind {
    /// Tauri (default v2, or v1 with use <tauri:v1>)
    Tauri,
    /// GTK (default v4, or v3 with use <gtk:3>)
    Gtk,
    /// Qt (default v6, or v5 with use <qt:5>)
    Qt,
    /// Android/Kotlin — future target (placeholder)
    Android,
    /// Slint UI framework
    Slint,
    /// Raw Rust crate from crates.io
    Ecosystem,
    /// npm/TypeScript/JavaScript package
    Npm,
    /// Vira registry (vira.io) — placeholder
    ViraRegistry,
}

#[derive(Debug, Clone)]
pub struct CrateDep {
    pub name: String,
    pub version: String,
    pub features: Vec<String>,
    pub optional: bool,
}

impl CrateDep {
    pub fn to_toml_entry(&self) -> String {
        let features_str = if self.features.is_empty() {
            String::new()
        } else {
            format!(
                ", features = [{}]",
                self.features
                .iter()
                .map(|f| format!("\"{}\"", f))
                .collect::<Vec<_>>()
                .join(", ")
            )
        };
        format!(
            "{} = {{ version = \"{}\"{}}}",
            self.name, self.version, features_str
        )
    }
}

/// Registry of known native APIs.
pub struct NativeApiRegistry {
    apis: HashMap<String, NativeApi>,
}

impl NativeApiRegistry {
    pub fn new() -> Self {
        let mut reg = NativeApiRegistry {
            apis: HashMap::new(),
        };
        reg.register_defaults();
        reg
    }

    fn register_defaults(&mut self) {
        // ── Tauri ──────────────────────────────────────────────────────────
        // This is the PRIMARY target for Vira / HackerOS
        self.apis.insert(
            "tauri".to_owned(),
                         NativeApi {
                             name: "tauri".to_owned(),
                         version: None,
                         kind: NativeApiKind::Tauri,
                         crates: vec![
                             CrateDep {
                                 name: "tauri".to_owned(),
                         version: "2".to_owned(),
                         features: vec![
                             "devtools".to_owned(),
                         ],
                         optional: false,
                             },
                             CrateDep {
                                 name: "tauri-build".to_owned(),
                         version: "2".to_owned(),
                         features: vec![],
                         optional: false,
                             },
                             CrateDep {
                                 name: "serde".to_owned(),
                         version: "1".to_owned(),
                         features: vec!["derive".to_owned()],
                         optional: false,
                             },
                             CrateDep {
                                 name: "serde_json".to_owned(),
                         version: "1".to_owned(),
                         features: vec![],
                         optional: false,
                             },
                         ],
                         rust_prelude: vec![
                             "use tauri::{AppHandle, Manager, State};".to_owned(),
                         "impl From<tauri::Error> for ViraError { fn from(e: tauri::Error) -> Self { ViraError::new(e.to_string()) } }".to_owned(),
                         "use serde::{Deserialize, Serialize};".to_owned(),
                         ],
                         features: vec!["tauri/devtools".to_owned()],
                         },
        );

        // ── GTK ────────────────────────────────────────────────────────────
        self.apis.insert(
            "gtk".to_owned(),
                         NativeApi {
                             name: "gtk".to_owned(),
                         version: None,
                         kind: NativeApiKind::Gtk,
                         crates: vec![
                             CrateDep {
                                 name: "gtk4".to_owned(),
                         version: "0.9".to_owned(),
                         features: vec!["v4_12".to_owned()],
                         optional: false,
                             },
                             CrateDep {
                                 name: "glib".to_owned(),
                         version: "0.20".to_owned(),
                         features: vec![],
                         optional: false,
                             },
                             CrateDep {
                                 name: "libadwaita".to_owned(),
                         version: "0.7".to_owned(),
                         features: vec![],
                         optional: true,
                             },
                         ],
                         rust_prelude: vec![
                             "use gtk4::prelude::*;".to_owned(),
                         "use gio::prelude::*;".to_owned(),
                         "use gio::ApplicationFlags;".to_owned(),
                         "use gtk4::prelude::*;".to_owned(),
                         "use gio::prelude::*;".to_owned(),
                         "use gio::ApplicationFlags;".to_owned(),
                         ],
                         features: vec![],
                         },
        );

        // ── Android / Mobile ───────────────────────────────────────────────
        // NOTE: Kotlin transpiler is a future milestone.
        // use <android>, use <mobile>, use <phone>, use <kotlin>
        // all map to this placeholder — vira will warn and emit a stub.
        for name in &["android", "mobile", "phone", "kotlin"] {
            self.apis.insert(
                name.to_string(),
                             NativeApi {
                                 name: name.to_string(),
                             version: None,
                             kind: NativeApiKind::Android,
                             crates: vec![],
                             rust_prelude: vec![
                                 "// [VIRA] Android/Kotlin target — transpiler coming soon".to_owned(),
                             "// This file is a placeholder. Kotlin output not yet implemented.".to_owned(),
                             ],
                             features: vec![],
                             },
            );
        }

        // ── Slint UI framework ────────────────────────────────────────────
        self.apis.insert(
            "slint".to_owned(),
                         NativeApi {
                             name: "slint".to_owned(),
                         version: None,
                         kind: NativeApiKind::Slint,
                         crates: vec![
                             CrateDep {
                                 name: "slint".to_owned(),
                         version: "1".to_owned(),
                         features: vec![],
                         optional: false,
                             },
                         ],
                         rust_prelude: vec![
                             "use slint::*;".to_owned(),
                         ],
                         features: vec![],
                         },
        );

        // ── Qt ─────────────────────────────────────────────────────────────
        self.apis.insert(
            "qt".to_owned(),
                         NativeApi {
                             name: "qt".to_owned(),
                         version: None,
                         kind: NativeApiKind::Qt,
                         crates: vec![
                             CrateDep {
                                 name: "cxx-qt".to_owned(),
                         version: "0.7".to_owned(),
                         features: vec![],
                         optional: false,
                             },
                             CrateDep {
                                 name: "cxx-qt-lib".to_owned(),
                         version: "0.7".to_owned(),
                         features: vec!["full".to_owned()],
                         optional: false,
                             },
                         ],
                         rust_prelude: vec![
                             "use cxx_qt_lib::QString;".to_owned(),
                         ],
                         features: vec![],
                         },
        );
    }

    /// Resolve a native API, applying version-specific overrides.
    /// Handles:
    ///   use <tauri>     → Tauri v2 (default)
    ///   use <tauri:v1>  → Tauri v1 (legacy)
    ///   use <gtk>       → GTK4 (default)
    ///   use <gtk:3>     → GTK3
    ///   use <qt>        → Qt6 (default)
    ///   use <qt:5>      → Qt5
    pub fn resolve(&self, name: &str, version: Option<&str>) -> Option<NativeApi> {
        let mut api = self.apis.get(name)?.clone();
        if let Some(v) = version {
            api.version = Some(v.to_owned());
            // Apply version-specific crate overrides
            match (name, v) {
                // Tauri v1 legacy
                ("tauri", "v1" | "1") => {
                    api.crates = vec![
                        CrateDep { name: "tauri".to_owned(), version: "1".to_owned(), features: vec!["api-all".to_owned()], optional: false },
                        CrateDep { name: "tauri-build".to_owned(), version: "1".to_owned(), features: vec![], optional: false },
                        CrateDep { name: "serde".to_owned(), version: "1".to_owned(), features: vec!["derive".to_owned()], optional: false },
                    ];
                    api.rust_prelude = vec![
                        "use tauri::{AppHandle, Manager, State, Window, command};".to_owned(),
                        "use serde::{Deserialize, Serialize};".to_owned(),
                    ];
                }
                // GTK3
                ("gtk", "3") => {
                    api.crates = vec![
                        CrateDep { name: "gtk".to_owned(), version: "0.18".to_owned(), features: vec![], optional: false },
                    ];
                    api.rust_prelude = vec![
                        "use gtk::prelude::*;".to_owned(),
                        "use gtk::{Application, ApplicationWindow, Button, Label};".to_owned(),
                    ];
                }
                // Qt5
                ("qt", "5") => {
                    api.crates = vec![
                        CrateDep { name: "cxx-qt".to_owned(), version: "0.6".to_owned(), features: vec![], optional: false },
                        CrateDep { name: "cxx-qt-lib".to_owned(), version: "0.6".to_owned(), features: vec![], optional: false },
                    ];
                }
                _ => {} // use default version from registry
            }
        }
        Some(api)
    }

    /// Resolve an ecosystem import: using <name> from <ecosystem>
    pub fn resolve_ecosystem(&self, name: &str, version: Option<&str>, ecosystem: &str) -> NativeApi {
        match ecosystem.to_lowercase().as_str() {
            "npm" | "typescript" | "javascript" | "ts" | "js" => {
                // npm/TypeScript package — emit comment in Rust, will be handled by UI layer
                NativeApi {
                    name: name.to_owned(),
                    version: version.map(|v| v.to_owned()),
                    kind: NativeApiKind::Npm,
                    crates: vec![], // no Rust crate
                    rust_prelude: vec![
                        format!("// [VIRA] npm package: {} {}", name, version.unwrap_or("latest")),
                            format!("// Included via lib/ UI layer — not a Rust dependency."),
                    ],
                    features: vec![],
                }
            }
            "vira" | "vira.io" => {
                NativeApi {
                    name: name.to_owned(),
                    version: version.map(|v| v.to_owned()),
                    kind: NativeApiKind::ViraRegistry,
                    crates: vec![],
                    rust_prelude: vec![
                        format!("// [VIRA] vira.io package: {} — registry coming soon", name),
                    ],
                    features: vec![],
                }
            }
            _ => {
                // Default: Rust crates.io
                NativeApi {
                    name: name.to_owned(),
                    version: version.map(|v| v.to_owned()),
                    kind: NativeApiKind::Ecosystem,
                    crates: vec![CrateDep {
                        name: name.to_owned(),
                        version: version.unwrap_or("*").to_owned(),
                        features: vec![],
                        optional: false,
                    }],
                    rust_prelude: vec![format!("use {};", name.replace('-', "_"))],
                    features: vec![],
                }
            }
        }
    }

    /// Legacy: resolve raw crate (used internally)
    pub fn resolve_crate(&self, name: &str, version: Option<&str>) -> NativeApi {
        self.resolve_ecosystem(name, version, "crates")
    }
}

impl Default for NativeApiRegistry {
    fn default() -> Self {
        NativeApiRegistry::new()
    }
}
