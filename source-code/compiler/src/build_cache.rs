use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use anyhow::{Context, Result};

/// Root of all Vira build output. Can be overridden with VIRA_BUILD_DIR.
pub fn build_root() -> PathBuf {
    std::env::var("VIRA_BUILD_DIR")
    .map(PathBuf::from)
    .unwrap_or_else(|_| PathBuf::from("/build"))
}

pub fn cache_root() -> PathBuf {
    build_root().join("cache")
}

pub fn project_out_dir(project: &str) -> PathBuf {
    build_root().join(project)
}

pub fn project_cache_dir(project: &str) -> PathBuf {
    cache_root().join(project)
}

// ─── Manifest ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheManifest {
    pub project: String,
    pub vira_version: String,
    pub created_at: u64,
    pub combined_hash: String,
    /// Map of source path → (hash, last_modified)
    pub sources: HashMap<String, SourceEntry>,
    /// Map of generated file → hash
    pub generated: HashMap<String, String>,
    pub profile: BuildProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BuildProfile {
    Debug,
    Release,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceEntry {
    pub hash: String,
    pub modified: u64,
    pub size: u64,
}

impl CacheManifest {
    pub fn new(project: &str, profile: BuildProfile) -> Self {
        CacheManifest {
            project: project.to_owned(),
            vira_version: env!("CARGO_PKG_VERSION").to_owned(),
            created_at: unix_now(),
            combined_hash: String::new(),
            sources: HashMap::new(),
            generated: HashMap::new(),
            profile,
        }
    }

    pub fn manifest_path(project: &str) -> PathBuf {
        project_cache_dir(project).join("manifest.json")
    }

    pub fn load(project: &str) -> Option<Self> {
        let path = Self::manifest_path(project);
        let data = std::fs::read_to_string(&path).ok()?;
        serde_json::from_str(&data).ok()
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::manifest_path(&self.project);
        std::fs::create_dir_all(path.parent().unwrap())?;
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json).context("writing cache manifest")
    }

    pub fn is_valid_for(&self, sources: &HashMap<String, SourceEntry>, profile: &BuildProfile) -> bool {
        if &self.profile != profile { return false; }
        if self.vira_version != env!("CARGO_PKG_VERSION") { return false; }
        if self.sources.len() != sources.len() { return false; }
        for (path, entry) in sources {
            match self.sources.get(path) {
                Some(cached) if cached.hash == entry.hash => {}
                _ => return false,
            }
        }
        true
    }
}

// ─── Cache manager ────────────────────────────────────────────────────────────

pub struct BuildCache {
    pub project: String,
    pub profile: BuildProfile,
}

impl BuildCache {
    pub fn new(project: impl Into<String>, release: bool) -> Self {
        BuildCache {
            project: project.into(),
            profile: if release { BuildProfile::Release } else { BuildProfile::Debug },
        }
    }

    /// Compute source entries for all given files.
    pub fn scan_sources(&self, files: &[PathBuf]) -> Result<HashMap<String, SourceEntry>> {
        let mut map = HashMap::new();
        for f in files {
            let meta = std::fs::metadata(f).with_context(|| format!("stat {}", f.display()))?;
            let data = std::fs::read(f).with_context(|| format!("read {}", f.display()))?;
            let hash = simple_hash(&data);
            let modified = meta.modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
            map.insert(
                f.to_string_lossy().to_string(),
                       SourceEntry { hash, modified, size: meta.len() },
            );
        }
        Ok(map)
    }

    /// Compute combined hash from all source entries.
    pub fn combined_hash(sources: &HashMap<String, SourceEntry>) -> String {
        let mut sorted: Vec<(&String, &SourceEntry)> = sources.iter().collect();
        sorted.sort_by_key(|(k, _)| k.as_str());
        let combined: String = sorted.iter().map(|(_, e)| e.hash.as_str()).collect::<Vec<_>>().join("|");
        simple_hash(combined.as_bytes())
    }

    /// Check if we have a valid cache hit.
    pub fn is_cached(&self, sources: &HashMap<String, SourceEntry>) -> bool {
        match CacheManifest::load(&self.project) {
            Some(manifest) => manifest.is_valid_for(sources, &self.profile),
            None => false,
        }
    }

    /// Return the cached Rust source if available.
    pub fn cached_rust_source(&self, combined_hash: &str) -> Option<String> {
        let path = project_cache_dir(&self.project).join(format!("{combined_hash}.rs"));
        std::fs::read_to_string(&path).ok()
    }

    /// Store transpiled Rust source in cache.
    pub fn store_rust_source(&self, combined_hash: &str, rust_source: &str) -> Result<()> {
        let dir = project_cache_dir(&self.project);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{combined_hash}.rs"));
        std::fs::write(&path, rust_source).context("storing cached rust source")
    }

    /// Store generated Cargo.toml in cache.
    pub fn store_cargo_toml(&self, cargo_toml: &str) -> Result<()> {
        let dir = project_cache_dir(&self.project);
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join("Cargo.toml.cached"), cargo_toml).context("storing cached Cargo.toml")
    }

    /// Prepare the output directory structure under /build/.
    pub fn prepare_out_dir(&self) -> Result<PathBuf> {
        let out = project_out_dir(&self.project);
        let rust_out = out.join("rust-src");
        std::fs::create_dir_all(rust_out.join("src"))?;

        // Cargo target dir goes inside /build/cache/<project>/cargo
        let cargo_target = project_cache_dir(&self.project).join("cargo-target");
        std::fs::create_dir_all(&cargo_target)?;

        Ok(rust_out)
    }

    /// Write final build output (binary) to /build/<project>/
    pub fn write_binary(&self, binary_path: &Path, _release: bool) -> Result<PathBuf> {
        let out_dir = project_out_dir(&self.project);
        std::fs::create_dir_all(&out_dir)?;
        let dest = out_dir.join(&self.project);
        std::fs::copy(binary_path, &dest)
        .with_context(|| format!("copying binary to {}", dest.display()))?;

        // make executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&dest)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&dest, perms)?;
        }

        Ok(dest)
    }

    /// Update and save the manifest after a successful build.
    pub fn update_manifest(
        &self,
        sources: HashMap<String, SourceEntry>,
        combined_hash: &str,
        generated_files: Vec<String>,
    ) -> Result<()> {
        let mut manifest = CacheManifest::new(&self.project, self.profile.clone());
        manifest.combined_hash = combined_hash.to_owned();
        manifest.sources = sources;
        for f in generated_files {
            let hash = std::fs::read(&f)
            .map(|d| simple_hash(&d))
            .unwrap_or_default();
            manifest.generated.insert(f, hash);
        }
        manifest.save()
    }

    /// Return CARGO_TARGET_DIR for this project's cache.
    pub fn cargo_target_dir(&self) -> PathBuf {
        project_cache_dir(&self.project).join("cargo-target")
    }

    /// Print a nice cache status line.
    pub fn print_status(&self, hit: bool) {
        let use_color = std::env::var("NO_COLOR").is_err();
        let green = if use_color { "\x1b[32m" } else { "" };
        let yellow = if use_color { "\x1b[33m" } else { "" };
        let reset = if use_color { "\x1b[0m" } else { "" };
        let dim = if use_color { "\x1b[2m" } else { "" };

        if hit {
            println!(
                "    {green}cache hit{reset}  {dim}{}",
                project_cache_dir(&self.project).display()
            );
        } else {
            println!(
                "    {yellow}cache miss{reset} {dim}{}",
                project_cache_dir(&self.project).display()
            );
        }
    }

    /// Evict old cache entries (keep last N hashes).
    pub fn evict_old_entries(&self, keep: usize) -> Result<()> {
        let dir = project_cache_dir(&self.project);
        if !dir.exists() { return Ok(()); }

        let mut rs_files: Vec<(PathBuf, SystemTime)> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "rs"))
        .filter_map(|e| {
            let mt = e.metadata().ok()?.modified().ok()?;
            Some((e.path(), mt))
        })
        .collect();

        rs_files.sort_by_key(|(_, mt)| *mt);

        // Remove oldest, keeping `keep` newest
        let to_remove = rs_files.len().saturating_sub(keep);
        for (path, _) in rs_files.iter().take(to_remove) {
            std::fs::remove_file(path).ok();
        }

        Ok(())
    }
}

// ─── Simple FNV-1a hash (no external dep) ────────────────────────────────────

fn simple_hash(data: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn unix_now() -> u64 {
    SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|d| d.as_secs())
    .unwrap_or(0)
}

// ─── Cache-aware compilation pipeline ────────────────────────────────────────

/// Result of a cache-aware build step.
pub enum CacheResult {
    Hit { rust_source: String, cargo_toml: String },
    Miss,
}

/// Run the full cache check for a project.
pub fn check_cache(project: &str, files: &[PathBuf], release: bool) -> Result<(CacheResult, BuildCache)> {
    let cache = BuildCache::new(project, release);
    let sources = cache.scan_sources(files)?;
    let hash = BuildCache::combined_hash(&sources);

    if cache.is_cached(&sources) {
        if let Some(rust) = cache.cached_rust_source(&hash) {
            let cargo = std::fs::read_to_string(
                project_cache_dir(project).join("Cargo.toml.cached")
            ).unwrap_or_default();
            cache.print_status(true);
            return Ok((CacheResult::Hit { rust_source: rust, cargo_toml: cargo }, cache));
        }
    }

    cache.print_status(false);
    Ok((CacheResult::Miss, cache))
}
