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
    Tauri,
    Gtk,
    Qt,
    Ecosystem, // `using` — raw rust crate
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
                            "protocol-asset".to_owned(),
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
                    "use tauri::{AppHandle, Manager, State, Window, Wry};".to_owned(),
                    "use tauri::command;".to_owned(),
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
                    "use gtk4::{Application, ApplicationWindow, Button, Label, Box as GtkBox};".to_owned(),
                    "use glib::clone;".to_owned(),
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
                    "use cxx_qt_lib::{QString, QObject};".to_owned(),
                ],
                features: vec![],
            },
        );
    }

    pub fn resolve(&self, name: &str, version: Option<&str>) -> Option<NativeApi> {
        let mut api = self.apis.get(name)?.clone();
        if let Some(v) = version {
            api.version = Some(v.to_owned());
        }
        Some(api)
    }

    pub fn resolve_crate(&self, name: &str, version: Option<&str>) -> NativeApi {
        // `using` → raw ecosystem crate
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

impl Default for NativeApiRegistry {
    fn default() -> Self {
        NativeApiRegistry::new()
    }
}
