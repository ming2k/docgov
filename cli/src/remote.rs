use anyhow::{Context, Result};
use colored::Colorize;
use flate2::read::GzDecoder;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use tar::Archive;

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct LatestReleaseInfo {
    pub tag_name: String,
    pub published_at: Option<String>,
    pub html_url: Option<String>,
}

pub fn parse_semver_parts(s: &str) -> (u64, u64, u64, &str) {
    let trimmed = s.trim();
    let clean = trimmed
        .strip_prefix("spec-v")
        .or_else(|| trimmed.strip_prefix("spec-"))
        .or_else(|| trimmed.strip_prefix("cli-v"))
        .or_else(|| trimmed.strip_prefix("cli-"))
        .or_else(|| trimmed.strip_prefix('v'))
        .unwrap_or(trimmed);
    let (num_part, pre) = match clean.find('-') {
        Some(idx) => (&clean[..idx], &clean[idx + 1..]),
        None => (clean, ""),
    };
    let mut parts = num_part.split('.');
    let major = parts
        .next()
        .and_then(|p| p.parse::<u64>().ok())
        .unwrap_or(0);
    let minor = parts
        .next()
        .and_then(|p| p.parse::<u64>().ok())
        .unwrap_or(0);
    let patch = parts
        .next()
        .and_then(|p| p.parse::<u64>().ok())
        .unwrap_or(0);
    (major, minor, patch, pre)
}

/// Compare two SemVer strings (e.g. "v0.1.0" vs "v0.1.1", or "0.0.8" vs "0.1.0").
pub fn compare_semver(a: &str, b: &str) -> std::cmp::Ordering {
    let (a_maj, a_min, a_pat, a_pre) = parse_semver_parts(a);
    let (b_maj, b_min, b_pat, b_pre) = parse_semver_parts(b);

    match (a_maj, a_min, a_pat).cmp(&(b_maj, b_min, b_pat)) {
        std::cmp::Ordering::Equal => match (a_pre.is_empty(), b_pre.is_empty()) {
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            _ => a_pre.cmp(b_pre),
        },
        ord => ord,
    }
}

/// Canonicalizes an upstream source specification URL.
///
/// In the monorepo architecture, governance specifications are sovereignly
/// hosted in `https://github.com/ming2k/docgov`. Legacy or alternate aliases
/// (`ming2k/docs-governance` or `ming2k/docgov-spec`) are transparently migrated
/// to `https://github.com/ming2k/docgov`.
pub fn canonicalize_upstream_source(source: &str) -> (String, bool) {
    let trimmed = source.trim().trim_end_matches('/');
    let stripped = trimmed
        .replace("https://github.com/", "")
        .replace("http://github.com/", "")
        .replace("github:", "");
    let parts: Vec<&str> = stripped.split('/').collect();
    if parts.len() >= 2 {
        let owner = parts[0];
        let repo = parts[1];
        if owner == "ming2k" && (repo == "docs-governance" || repo == "docgov-spec") {
            return ("https://github.com/ming2k/docgov".to_string(), true);
        }
        if owner == "ming2k" && repo == "docgov" {
            return ("https://github.com/ming2k/docgov".to_string(), false);
        }
    }
    (trimmed.to_string(), false)
}

pub struct RemoteClient {
    pub source: String,
    pub r#ref: String,
}

impl RemoteClient {
    pub fn new(source: &str, r#ref: &str) -> Self {
        let (canonical_source, _) = canonicalize_upstream_source(source);
        Self {
            source: canonical_source,
            r#ref: r#ref.to_string(),
        }
    }

    /// Resolve the latest available specification release from upstream.
    ///
    /// Resilience strategy:
    /// 1. If local spec is available (e.g. sibling `../docgov-spec` or local path), reads `VERSION` file.
    /// 2. Probes GitHub web redirect `https://github.com/{owner}/{repo}/releases/latest` (resilient, no API rate-limits).
    /// 3. Falls back to GitHub REST API `/repos/{owner}/{repo}/releases/latest`.
    pub fn fetch_latest_release(&self, workspace_root: Option<&Path>) -> Result<LatestReleaseInfo> {
        // 1. Check local spec first if available
        if let Some(ws) = workspace_root {
            if let Some(spec_dir) = self.local_spec_dir(ws) {
                let version_file = spec_dir.join("VERSION");
                if version_file.is_file() {
                    let ver = fs::read_to_string(&version_file)?.trim().to_string();
                    let tag = if ver.starts_with('v') {
                        ver
                    } else {
                        format!("v{}", ver)
                    };
                    return Ok(LatestReleaseInfo {
                        tag_name: tag,
                        published_at: None,
                        html_url: Some(format!("file://{}", spec_dir.display())),
                    });
                }
            }
        }

        let (owner, repo) = self.parse_owner_repo();

        let is_explicit_spec_tag = |tag: &str| -> bool {
            let t = tag.trim();
            t.starts_with("spec-")
        };

        let normalize_tag = |raw: &str| -> String {
            let trimmed = raw.trim();
            if let Some(stripped) = trimmed.strip_prefix("spec-") {
                if stripped.starts_with('v') {
                    stripped.to_string()
                } else {
                    format!("v{}", stripped)
                }
            } else if trimmed.starts_with('v') {
                trimmed.to_string()
            } else {
                format!("v{}", trimmed)
            }
        };

        // 2. Probe GitHub Web redirect (zero rate-limit, standard 302 location)
        // If the redirect location explicitly points to a spec-* tag, we use it directly.
        let web_url = format!("https://github.com/{}/{}/releases/latest", owner, repo);
        if let Ok(agent) = ureq::AgentBuilder::new()
            .timeout(std::time::Duration::from_secs(5))
            .redirects(0)
            .build()
            .get(&web_url)
            .set("User-Agent", "docgov-cli")
            .call()
        {
            if agent.status() == 302 {
                if let Some(loc) = agent.header("Location") {
                    if let Some(tag) = loc.split("/releases/tag/").nth(1) {
                        if is_explicit_spec_tag(tag) {
                            return Ok(LatestReleaseInfo {
                                tag_name: normalize_tag(tag),
                                published_at: None,
                                html_url: Some(loc.to_string()),
                            });
                        }
                    }
                }
            }
        }

        // 3. Fallback to GitHub REST API: query releases list to find latest specification release
        // (especially vital for monorepos where /releases/latest is the CLI engine release).
        let api_url = format!(
            "https://api.github.com/repos/{}/{}/releases?per_page=20",
            owner, repo
        );
        let resp = ureq::get(&api_url)
            .timeout(std::time::Duration::from_secs(5))
            .set("User-Agent", "docgov-cli")
            .set("Accept", "application/vnd.github.v3+json")
            .call()
            .with_context(|| format!("Failed to fetch releases from {}", api_url))?;

        let releases: Vec<serde_json::Value> = serde_json::from_reader(resp.into_reader())?;
        // Pass 1: Look for explicit spec-* tag in monorepo releases
        for rel in &releases {
            if let Some(raw_tag) = rel["tag_name"].as_str() {
                if is_explicit_spec_tag(raw_tag) {
                    let published_at = rel["published_at"].as_str().map(|s| s.to_string());
                    let html_url = rel["html_url"].as_str().map(|s| s.to_string());
                    return Ok(LatestReleaseInfo {
                        tag_name: normalize_tag(raw_tag),
                        published_at,
                        html_url,
                    });
                }
            }
        }

        // Pass 2: In case upstream is a dedicated standalone spec repository (standard v* tags),
        // fallback to the first release not prefixed with "cli-"
        for rel in &releases {
            if let Some(raw_tag) = rel["tag_name"].as_str() {
                let trimmed = raw_tag.trim();
                if !trimmed.starts_with("cli-") {
                    let published_at = rel["published_at"].as_str().map(|s| s.to_string());
                    let html_url = rel["html_url"].as_str().map(|s| s.to_string());
                    return Ok(LatestReleaseInfo {
                        tag_name: normalize_tag(raw_tag),
                        published_at,
                        html_url,
                    });
                }
            }
        }

        anyhow::bail!("No specification release found for {}/{}", owner, repo)
    }

    /// Resolve latest upstream release with a local TTL cache (default 6 hours).
    ///
    /// This guarantees that repeated `docgov check` invocations never hammer the network,
    /// while still delivering timely update awareness. Cache is stored per-source under
    /// the XDG-compliant docgov cache root. CI environments skip the notifier entirely.
    pub fn fetch_latest_release_cached(
        &self,
        workspace_root: Option<&Path>,
        ttl_secs: u64,
    ) -> Result<LatestReleaseInfo> {
        let cache_root = self.get_cache_dir();
        let cache_file = cache_root.join("latest_release.json");

        // 1. Attempt fresh cache hit
        if let Ok(cached_text) = fs::read_to_string(&cache_file) {
            if let Ok(cached) = serde_json::from_str::<LatestReleaseInfo>(&cached_text) {
                if let Ok(meta) = fs::metadata(&cache_file) {
                    if let Ok(modified) = meta.modified() {
                        if let Ok(age) = modified.elapsed() {
                            if age.as_secs() < ttl_secs {
                                return Ok(cached);
                            }
                        }
                    }
                }
            }
        }

        // 2. Cache miss / stale: resolve fresh and persist (best-effort)
        let info = self.fetch_latest_release(workspace_root)?;
        if let Ok(serialized) = serde_json::to_string_pretty(&info) {
            let _ = fs::create_dir_all(&cache_root);
            let _ = fs::write(&cache_file, serialized);
        }
        Ok(info)
    }

    /// Check whether source specifies a local filesystem path
    pub fn local_spec_dir(&self, workspace_root: &Path) -> Option<PathBuf> {
        let path_str = self.source.strip_prefix("file://").unwrap_or(&self.source);
        if path_str == "." || path_str == "local" || path_str == "self" {
            let ws_spec = workspace_root.join("spec");
            if ws_spec.is_dir() {
                return Some(ws_spec);
            }
            if let Ok(cur) = std::env::current_dir() {
                let cur_spec = cur.join("spec");
                if cur_spec.is_dir() {
                    return Some(cur_spec);
                }
                if let Some(parent) = cur.parent() {
                    let sibling_spec = parent.join("docgov-spec");
                    if sibling_spec.join("core").is_dir() {
                        return Some(sibling_spec);
                    }
                    let monorepo_spec = parent.join("spec");
                    if monorepo_spec.join("core").is_dir() {
                        return Some(monorepo_spec);
                    }
                }
            }
            if let Some(parent) = workspace_root.parent() {
                let sibling_spec = parent.join("docgov-spec");
                if sibling_spec.join("core").is_dir() {
                    return Some(sibling_spec);
                }
                let monorepo_spec = parent.join("spec");
                if monorepo_spec.join("core").is_dir() {
                    return Some(monorepo_spec);
                }
            }
            if workspace_root.join("core").is_dir() && workspace_root.join("profiles").is_dir() {
                return Some(workspace_root.to_path_buf());
            }
        }
        let direct_path = Path::new(path_str);
        if direct_path.is_dir() {
            if direct_path.join("spec").is_dir() {
                return Some(direct_path.join("spec"));
            }
            return Some(direct_path.to_path_buf());
        }
        let ws_relative = workspace_root.join(path_str);
        if ws_relative.is_dir() {
            if ws_relative.join("spec").is_dir() {
                return Some(ws_relative.join("spec"));
            }
            return Some(ws_relative);
        }
        None
    }

    /// Determine local cache directory for this specific upstream and ref.
    /// Strictly complies with the XDG Base Directory Specification:
    /// 1. Prioritizes `$XDG_CACHE_HOME/docgov` if set and non-empty.
    /// 2. Falls back to OS-native cache directory via `dirs::cache_dir()`.
    /// 3. Safely falls back to `~/.cache/docgov` if environment lookup fails.
    pub fn get_cache_dir(&self) -> PathBuf {
        let base = if let Some(xdg) = std::env::var_os("XDG_CACHE_HOME") {
            if !xdg.is_empty() {
                PathBuf::from(xdg).join("docgov")
            } else {
                dirs::cache_dir()
                    .unwrap_or_else(fallback_cache_dir)
                    .join("docgov")
            }
        } else {
            dirs::cache_dir()
                .unwrap_or_else(fallback_cache_dir)
                .join("docgov")
        };

        // Sanitize source identifier to make safe path
        let sanitized_source = self
            .source
            .replace("https://", "")
            .replace("http://", "")
            .replace("github.com/", "")
            .replace("github:", "")
            .replace(['/', ':', '@'], "_");

        base.join(sanitized_source).join(&self.r#ref)
    }

    /// Fetch config template with local-spec, cache-first, and remote-network strategy.
    pub fn fetch_config_template(&self, workspace_root: Option<&Path>) -> Result<(String, String)> {
        // 1. Local workspace specification source check
        if let Some(ws) = workspace_root {
            if let Some(spec_dir) = self.local_spec_dir(ws) {
                let candidate_file = if spec_dir.join("default-config.yml").is_file() {
                    spec_dir.join("default-config.yml")
                } else if spec_dir.join("spec/default-config.yml").is_file() {
                    spec_dir.join("spec/default-config.yml")
                } else {
                    PathBuf::new()
                };
                if candidate_file.is_file() {
                    let content = fs::read_to_string(&candidate_file).with_context(|| {
                        format!(
                            "Failed to read local default config template at {}",
                            candidate_file.display()
                        )
                    })?;
                    return Ok((content, format!("local:{}", candidate_file.display())));
                }
            }
        }

        let cache_dir = self.get_cache_dir();
        let cache_file = cache_dir.join("default-config.yml");

        // 2. Cache hit check
        if cache_file.exists() {
            if let Ok(content) = fs::read_to_string(&cache_file) {
                if !content.trim().is_empty() {
                    return Ok((content, "cache".to_string()));
                }
            }
        }

        // 3. Network fetch attempt
        let download_urls = self.candidate_config_download_urls();
        for url in &download_urls {
            if let Ok(content) = self.download_url(url) {
                if !content.trim().is_empty() {
                    let _ = fs::create_dir_all(&cache_dir);
                    let _ = fs::write(&cache_file, &content);
                    return Ok((content, url.clone()));
                }
            }
        }

        anyhow::bail!(
            "Failed to fetch default config template for upstream '{}' (ref: '{}').",
            self.source,
            self.r#ref
        );
    }

    /// Fetch directives snippet with local-spec, cache-first, and remote-network strategy.
    /// Fails deterministically if the asset is not reachable.
    pub fn fetch_directives(&self, workspace_root: Option<&Path>) -> Result<(String, String)> {
        // 1. Local workspace specification source check
        if let Some(ws) = workspace_root {
            if let Some(spec_dir) = self.local_spec_dir(ws) {
                let candidate_file = if spec_dir.join("directives.snippet").is_file() {
                    spec_dir.join("directives.snippet")
                } else if spec_dir.join("spec/directives.snippet").is_file() {
                    spec_dir.join("spec/directives.snippet")
                } else {
                    anyhow::bail!(
                        "Local governance directory '{}' does not contain 'directives.snippet'.",
                        spec_dir.display()
                    );
                };
                let content = fs::read_to_string(&candidate_file).with_context(|| {
                    format!(
                        "Failed to read local snippet at {}",
                        candidate_file.display()
                    )
                })?;
                println!(
                    "{} Using local specification directives ({})",
                    "✔".green().bold(),
                    candidate_file.display()
                );
                return Ok((content, format!("local:{}", candidate_file.display())));
            }
        }

        let cache_dir = self.get_cache_dir();
        let cache_file = cache_dir.join("directives.snippet");

        // 2. Cache hit check: if cached locally, use immediately without network call
        if cache_file.exists() {
            if let Ok(content) = fs::read_to_string(&cache_file) {
                if !content.trim().is_empty() {
                    println!(
                        "{} Cached remote assets hit ({})",
                        "✔".green().bold(),
                        cache_file.display()
                    );
                    return Ok((content, "cache".to_string()));
                }
            }
        }

        // 3. Network fetch attempt
        let download_urls = self.candidate_download_urls();
        for url in &download_urls {
            match self.download_url(url) {
                Ok(content) if !content.trim().is_empty() => {
                    // Save to local cache for future runs
                    let _ = fs::create_dir_all(&cache_dir);
                    let _ = fs::write(&cache_file, &content);
                    println!(
                        "{} Fetched remote directives from {}",
                        "✔".green().bold(),
                        url
                    );
                    return Ok((content, url.clone()));
                }
                _ => continue,
            }
        }

        // 4. Deterministic failure when unresolvable
        let attempted_list = download_urls
            .iter()
            .map(|u| format!("- {}", u))
            .collect::<Vec<_>>()
            .join("\n");
        anyhow::bail!(
            "Failed to fetch directives snippet for upstream '{}' (ref: '{}').\n\
             Endpoints attempted:\n\
             {}\n\
             Asset not found in local cache or remote endpoints. Please check network connectivity or upstream release status.",
            self.source,
            self.r#ref,
            attempted_list
        );
    }

    /// Atomically sync and replace the canonical governance documentation mirror
    /// completely pruning old/deleted files from prior versions.
    pub fn sync_governance_docs(
        &self,
        workspace_root: &Path,
        target_rel_path: &str,
        force: bool,
    ) -> Result<String> {
        let target_dir = workspace_root.join(target_rel_path);

        // 1. Local spec source: sync directly from local directory
        if let Some(spec_dir) = self.local_spec_dir(workspace_root) {
            return self.sync_from_local_dir(&spec_dir, &target_dir, workspace_root);
        }

        // 2. Remote spec source: fetch release archive or read cache
        let cache_dir = self.get_cache_dir();
        let cache_tar = cache_dir.join("docgov-assets.tar.gz");

        let tar_bytes: Vec<u8> = if !force && cache_tar.exists() {
            println!(
                "{} Using cached governance documentation archive ({})",
                "✔".green().bold(),
                cache_tar.display()
            );
            fs::read(&cache_tar)?
        } else {
            let (owner, repo) = self.parse_owner_repo();
            let clean_ver = self
                .r#ref
                .strip_prefix("spec-v")
                .or_else(|| self.r#ref.strip_prefix("spec-"))
                .or_else(|| self.r#ref.strip_prefix('v'))
                .unwrap_or(&self.r#ref);
            let candidate_tar_urls = vec![
                format!(
                    "https://github.com/{}/{}/releases/download/spec-v{}/docgov-spec-{}.tar.gz",
                    owner, repo, clean_ver, clean_ver
                ),
                format!(
                    "https://github.com/{}/{}/releases/download/spec-v{}/dog-spec-{}.tar.gz",
                    owner, repo, clean_ver, clean_ver
                ),
                format!(
                    "https://github.com/{}/{}/releases/download/spec-v{}/docgov-assets.tar.gz",
                    owner, repo, clean_ver
                ),
                format!(
                    "https://github.com/{}/{}/releases/download/{}/docgov-spec-{}.tar.gz",
                    owner, repo, self.r#ref, clean_ver
                ),
                format!(
                    "https://github.com/{}/{}/releases/download/{}/dog-spec-{}.tar.gz",
                    owner, repo, self.r#ref, clean_ver
                ),
                format!(
                    "https://github.com/{}/{}/releases/download/{}/docgov-assets.tar.gz",
                    owner, repo, self.r#ref
                ),
            ];

            let mut last_err = None;
            let mut downloaded = None;

            for url in &candidate_tar_urls {
                match self.download_bytes(url) {
                    Ok(bytes) => {
                        let _ = fs::create_dir_all(&cache_dir);
                        let _ = fs::write(&cache_tar, &bytes);
                        println!(
                            "{} Downloaded governance documentation assets from {}",
                            "✔".green().bold(),
                            url
                        );
                        downloaded = Some(bytes);
                        break;
                    }
                    Err(err) => {
                        last_err = Some(format!("URL: {} -> {}", url, err));
                    }
                }
            }

            match downloaded {
                Some(bytes) => bytes,
                None => {
                    anyhow::bail!(
                        "Failed to download governance documentation assets for upstream '{}/{}' (ref: '{}').\n\
                         Details:\n{}\n\
                         Release asset 'docgov-spec-{}.tar.gz' or 'docgov-assets.tar.gz' is unreachable. Ensure the release exists and assets are published.",
                        owner,
                        repo,
                        self.r#ref,
                        last_err.unwrap_or_default(),
                        clean_ver
                    );
                }
            }
        };

        // Compute SHA-256 fingerprint of the tarball
        let archive_hash = crate::lockfile::compute_sha256_bytes(&tar_bytes);

        // Temporary staging directory for atomic unpack
        let staging_dir = target_dir
            .parent()
            .unwrap_or(workspace_root)
            .join(format!(".tmp_sync_{}", std::process::id()));
        if staging_dir.exists() {
            let _ = fs::remove_dir_all(&staging_dir);
        }
        fs::create_dir_all(&staging_dir)?;

        // Unpack tar.gz into staging_dir, stripping leading "spec/"
        let gz = GzDecoder::new(&tar_bytes[..]);
        let mut archive = Archive::new(gz);

        for entry_res in archive.entries()? {
            let mut entry = entry_res?;
            let path = entry.path()?;
            let path_str = path.to_string_lossy();

            let rel_path = if let Some(stripped) = path_str.strip_prefix("spec/") {
                stripped.to_string()
            } else if path_str == "spec" {
                continue;
            } else {
                path_str.to_string()
            };

            if rel_path.is_empty()
                || rel_path == "directives.snippet"
                || rel_path.starts_with(".git")
                || rel_path.starts_with(".github")
            {
                continue;
            }

            let dest_file = staging_dir.join(&rel_path);
            if entry.header().entry_type().is_dir() {
                fs::create_dir_all(&dest_file)?;
            } else {
                if let Some(parent) = dest_file.parent() {
                    fs::create_dir_all(parent)?;
                }
                entry.unpack(&dest_file)?;
            }
        }

        // Full atomic mirror replacement: prune old target completely to eliminate ghost files
        if target_dir.exists() {
            fs::remove_dir_all(&target_dir)?;
        }
        if let Some(parent) = target_dir.parent() {
            fs::create_dir_all(parent)?;
        }

        if fs::rename(&staging_dir, &target_dir).is_err() {
            copy_dir_all(&staging_dir, &target_dir, true)?;
            let _ = fs::remove_dir_all(&staging_dir);
        }

        println!(
            "{} Atomically synchronized and pruned canonical governance documentation into {}",
            "✔".green().bold(),
            target_dir.display()
        );

        Ok(archive_hash)
    }

    fn sync_from_local_dir(
        &self,
        spec_dir: &Path,
        target_dir: &Path,
        workspace_root: &Path,
    ) -> Result<String> {
        let staging_dir = target_dir
            .parent()
            .unwrap_or(workspace_root)
            .join(format!(".tmp_sync_{}", std::process::id()));
        if staging_dir.exists() {
            let _ = fs::remove_dir_all(&staging_dir);
        }
        fs::create_dir_all(&staging_dir)?;

        copy_dir_all(spec_dir, &staging_dir, false)?;

        if target_dir.exists() {
            fs::remove_dir_all(target_dir)?;
        }
        if let Some(parent) = target_dir.parent() {
            fs::create_dir_all(parent)?;
        }

        if fs::rename(&staging_dir, target_dir).is_err() {
            copy_dir_all(&staging_dir, target_dir, true)?;
            let _ = fs::remove_dir_all(&staging_dir);
        }

        let hash = compute_dir_sha256(target_dir)?;
        println!(
            "{} Atomically synchronized local governance specification into {}",
            "✔".green().bold(),
            target_dir.display()
        );

        Ok(hash)
    }

    fn candidate_config_download_urls(&self) -> Vec<String> {
        let (owner, repo) = self.parse_owner_repo();
        let clean_ver = self
            .r#ref
            .strip_prefix("spec-v")
            .or_else(|| self.r#ref.strip_prefix("spec-"))
            .or_else(|| self.r#ref.strip_prefix('v'))
            .unwrap_or(&self.r#ref);
        vec![
            format!(
                "https://github.com/{}/{}/releases/download/spec-v{}/default-config.yml",
                owner, repo, clean_ver
            ),
            format!(
                "https://github.com/{}/{}/releases/download/{}/default-config.yml",
                owner, repo, self.r#ref
            ),
            format!(
                "https://raw.githubusercontent.com/{}/{}/spec-v{}/spec/default-config.yml",
                owner, repo, clean_ver
            ),
            format!(
                "https://raw.githubusercontent.com/{}/{}/{}/spec/default-config.yml",
                owner, repo, self.r#ref
            ),
            format!(
                "https://raw.githubusercontent.com/{}/{}/{}/default-config.yml",
                owner, repo, self.r#ref
            ),
        ]
    }

    fn candidate_download_urls(&self) -> Vec<String> {
        let (owner, repo) = self.parse_owner_repo();
        let clean_ver = self
            .r#ref
            .strip_prefix("spec-v")
            .or_else(|| self.r#ref.strip_prefix("spec-"))
            .or_else(|| self.r#ref.strip_prefix('v'))
            .unwrap_or(&self.r#ref);
        vec![
            // 1. GitHub Release asset under spec-v* tag (canonical monorepo release)
            format!(
                "https://github.com/{}/{}/releases/download/spec-v{}/directives.snippet",
                owner, repo, clean_ver
            ),
            // 2. GitHub Release asset under given ref tag
            format!(
                "https://github.com/{}/{}/releases/download/{}/directives.snippet",
                owner, repo, self.r#ref
            ),
            // 3. Raw GitHub content under spec-v* tag (docgov monorepo layout)
            format!(
                "https://raw.githubusercontent.com/{}/{}/spec-v{}/spec/directives.snippet",
                owner, repo, clean_ver
            ),
            // 4. Raw GitHub content in spec/ subdirectory (docgov monorepo layout)
            format!(
                "https://raw.githubusercontent.com/{}/{}/{}/spec/directives.snippet",
                owner, repo, self.r#ref
            ),
            // 5. Direct root directives.snippet in standalone repository
            format!(
                "https://raw.githubusercontent.com/{}/{}/{}/directives.snippet",
                owner, repo, self.r#ref
            ),
        ]
    }

    fn parse_owner_repo(&self) -> (String, String) {
        let stripped = self
            .source
            .replace("https://github.com/", "")
            .replace("http://github.com/", "")
            .replace("github:", "");
        let parts: Vec<&str> = stripped.split('/').collect();
        if parts.len() >= 2 {
            (parts[0].to_string(), parts[1].to_string())
        } else {
            ("ming2k".to_string(), "docgov".to_string())
        }
    }

    fn download_url(&self, url: &str) -> Result<String> {
        let response = ureq::get(url)
            .timeout(std::time::Duration::from_secs(5))
            .call()?;
        let body = response.into_string()?;
        Ok(body)
    }

    fn download_bytes(&self, url: &str) -> Result<Vec<u8>> {
        let response = ureq::get(url)
            .timeout(std::time::Duration::from_secs(10))
            .call()?;
        let mut reader = response.into_reader();
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Ok(bytes)
    }
}

fn copy_dir_all(src: &Path, dst: &Path, include_directives: bool) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy();
        if name_str.starts_with(".git")
            || name_str.starts_with(".github")
            || (!include_directives && name_str == "directives.snippet")
        {
            continue;
        }
        let to = dst.join(file_name);
        if ty.is_dir() {
            copy_dir_all(&from, &to, include_directives)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

fn compute_dir_sha256(dir: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    let mut entries = Vec::new();
    for entry in walkdir::WalkDir::new(dir).sort_by_file_name() {
        let entry = entry?;
        if entry.file_type().is_file() {
            entries.push(entry.into_path());
        }
    }
    entries.sort();
    for path in entries {
        let rel = path.strip_prefix(dir).unwrap_or(&path);
        hasher.update(rel.to_string_lossy().as_bytes());
        let content = fs::read(&path)?;
        hasher.update(&content);
    }
    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}

fn fallback_cache_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".cache")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semver_comparison() {
        use std::cmp::Ordering;
        assert_eq!(compare_semver("v0.1.0", "v0.1.0"), Ordering::Equal);
        assert_eq!(compare_semver("v0.1.0", "v0.1.1"), Ordering::Less);
        assert_eq!(compare_semver("v0.2.0", "v0.1.9"), Ordering::Greater);
        assert_eq!(compare_semver("0.0.8", "0.1.0"), Ordering::Less);
        assert_eq!(compare_semver("v1.0.0-rc1", "v1.0.0"), Ordering::Less);
        assert_eq!(compare_semver("v1.0.0", "v1.0.0-rc1"), Ordering::Greater);
    }
}
