use std::path::{Component, Path, PathBuf};

/// Normalize a path removing '.' and resolving '..' without accessing the filesystem
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if let Some(Component::Normal(_)) = components.last() {
                    components.pop();
                } else {
                    components.push(component);
                }
            }
            _ => components.push(component),
        }
    }
    components.iter().collect()
}

/// Resolve a relative link from a base markdown file
pub fn resolve_relative_link<P: AsRef<Path>>(base_file: P, link_dest: &str) -> Option<PathBuf> {
    // Strip anchors (#...) and query strings (?...)
    let clean_dest = link_dest.split('#').next()?.split('?').next()?;
    if clean_dest.is_empty() {
        return None;
    }

    // Ignore absolute URLs and mailto
    if clean_dest.starts_with("http://")
        || clean_dest.starts_with("https://")
        || clean_dest.starts_with("mailto:")
        || clean_dest.starts_with("//")
    {
        return None;
    }

    let base_dir = base_file.as_ref().parent().unwrap_or_else(|| Path::new(""));
    let combined = base_dir.join(clean_dest);
    Some(normalize_path(&combined))
}

/// Match path against a glob pattern string (e.g. "docs/dev/**")
pub fn matches_glob(path: &Path, pattern: &str) -> bool {
    let path_str = path.to_string_lossy().replace('\\', "/");
    let clean_pattern = pattern.replace('\\', "/");
    if let Ok(glob_pattern) = glob::Pattern::new(&clean_pattern) {
        glob_pattern.matches(&path_str)
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path() {
        assert_eq!(
            normalize_path(Path::new("docs/tutorials/../dev/setup.md")),
            PathBuf::from("docs/dev/setup.md")
        );
        assert_eq!(
            normalize_path(Path::new("a/b/c/../../d.md")),
            PathBuf::from("a/d.md")
        );
    }

    #[test]
    fn test_resolve_relative_link() {
        let base = Path::new("docs/tutorials/getting-started.md");
        let target = resolve_relative_link(base, "../dev/setup.md#local-build");
        assert_eq!(target, Some(PathBuf::from("docs/dev/setup.md")));

        // External links should be ignored
        assert_eq!(resolve_relative_link(base, "https://example.com"), None);
        assert_eq!(resolve_relative_link(base, "mailto:test@example.com"), None);
        assert_eq!(resolve_relative_link(base, "#section-header"), None);
    }

    #[test]
    fn test_matches_glob() {
        assert!(matches_glob(Path::new("docs/dev/setup.md"), "docs/dev/**"));
        assert!(matches_glob(
            Path::new("docs/tutorials/intro.md"),
            "docs/tutorials/**"
        ));
        assert!(!matches_glob(
            Path::new("docs/tutorials/intro.md"),
            "docs/dev/**"
        ));
    }
}
