use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn get_changed_files<P: AsRef<Path>>(
    workspace_root: P,
    base_ref: Option<&str>,
) -> Result<Vec<PathBuf>> {
    let root = workspace_root.as_ref();

    let output = if let Some(base) = base_ref {
        Command::new("git")
            .arg("diff")
            .arg("--name-only")
            .arg(base)
            .current_dir(root)
            .output()
            .with_context(|| format!("Failed to run 'git diff --name-only {}'", base))?
    } else {
        // Collect both staged, unstaged, and untracked files
        let staged = Command::new("git")
            .arg("diff")
            .arg("--name-only")
            .arg("HEAD")
            .current_dir(root)
            .output();

        match staged {
            Ok(out) if out.status.success() => out,
            _ => {
                // Fallback to git status --porcelain
                Command::new("git")
                    .arg("status")
                    .arg("--porcelain")
                    .current_dir(root)
                    .output()
                    .context("Failed to run 'git status --porcelain'")?
            }
        }
    };

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut files = Vec::new();

    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Handle git status --porcelain format (XY filename)
        let file_path = if trimmed.len() > 3
            && (trimmed.as_bytes()[2] == b' ' || trimmed.as_bytes()[1] == b' ')
        {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 2 {
                parts[1]
            } else {
                trimmed
            }
        } else {
            trimmed
        };

        files.push(PathBuf::from(file_path));
    }

    Ok(files)
}
