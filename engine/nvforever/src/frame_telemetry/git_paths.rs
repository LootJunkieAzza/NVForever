//! Git metadata paths for build provenance (docs/design/frame-telemetry.md §2).

use std::path::{Component, Path, PathBuf};

pub struct GitPaths {
    pub head: PathBuf,
    pub branch: Option<PathBuf>,
    pub packed_refs: Vec<PathBuf>,
    common: PathBuf,
}

impl GitPaths {
    /// A packed-only branch needs a directory watch to observe its first loose ref.
    pub fn branch_watch(&self) -> Option<PathBuf> {
        let branch = self.branch.as_deref()?;
        if branch.is_file() {
            return Some(branch.to_path_buf());
        }
        let mut ancestor = branch.parent()?;
        while ancestor.starts_with(&self.common) {
            if ancestor.is_dir() {
                return Some(ancestor.to_path_buf());
            }
            ancestor = ancestor.parent()?;
        }
        None
    }
}

/// Git's relative directory answers are relative to the command's working directory.
pub fn resolve(base: &Path, git_dir: &str, common_dir: &str, symbolic: &str) -> Option<GitPaths> {
    let absolute = |value: &str| {
        if value.is_empty() {
            return None;
        }
        let path = Path::new(value);
        Some(if path.is_absolute() { path.to_path_buf() } else { base.join(path) })
    };
    let git = absolute(git_dir)?;
    let common = absolute(common_dir)?;
    let branch = if symbolic == "HEAD" {
        None
    } else {
        if !symbolic.starts_with("refs/") || !Path::new(symbolic).components().all(|part| matches!(part, Component::Normal(_))) {
            return None;
        }
        Some(common.join(symbolic))
    };
    let mut packed_refs = vec![git.join("packed-refs")];
    if common != git {
        packed_refs.push(common.join("packed-refs"));
    }
    Some(GitPaths {
        head: git.join("HEAD"),
        branch,
        packed_refs,
        common,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let timestamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
            let path = std::env::temp_dir().join(format!(
                "nvf-git-paths-{}-{timestamp}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn packed_branch_watches_nearest_existing_ancestor_then_loose_ref() {
        let temp = TempDir::new();
        let common = temp.0.join("common");
        std::fs::create_dir_all(common.join("refs/heads/codex")).unwrap();
        let paths = resolve(&temp.0, ".git", common.to_str().unwrap(), "refs/heads/codex/topic").unwrap();
        assert_eq!(paths.branch_watch(), Some(common.join("refs/heads/codex")));
        std::fs::write(paths.branch.as_ref().unwrap(), "synthetic ref").unwrap();
        assert_eq!(paths.branch_watch(), paths.branch);
    }

    #[test]
    fn packed_branch_can_watch_common_directory_without_leaving_it() {
        let temp = TempDir::new();
        let paths = resolve(&temp.0, ".git", temp.0.to_str().unwrap(), "refs/heads/missing/topic").unwrap();
        assert_eq!(paths.branch_watch(), Some(temp.0.clone()));
        assert!(resolve(&temp.0, ".git", "nonexistent", "refs/heads/topic").unwrap().branch_watch().is_none());
        assert!(resolve(&temp.0, ".git", temp.0.to_str().unwrap(), "HEAD").unwrap().branch_watch().is_none());
    }

    #[test]
    fn regular_checkout_and_detached_head() {
        let base = std::env::temp_dir();
        let paths = resolve(&base, ".git", ".git", "refs/heads/main").unwrap();
        assert_eq!(paths.head, base.join(".git/HEAD"));
        assert_eq!(paths.branch, Some(base.join(".git/refs/heads/main")));
        assert_eq!(paths.packed_refs, vec![base.join(".git/packed-refs")]);
        assert!(resolve(&base, ".git", ".git", "HEAD").unwrap().branch.is_none());
    }

    #[test]
    fn worktree_uses_common_branch_and_both_packed_refs_candidates() {
        let base = std::env::temp_dir();
        let git = base.join("clone/.git/worktrees/performance");
        let common = base.join("clone/.git");
        let paths = resolve(&base, git.to_str().unwrap(), common.to_str().unwrap(), "refs/heads/codex/performance").unwrap();
        assert_eq!(paths.head, git.join("HEAD"));
        assert_eq!(paths.branch, Some(common.join("refs/heads/codex/performance")));
        assert_eq!(paths.packed_refs, vec![git.join("packed-refs"), common.join("packed-refs")]);
    }

    #[test]
    fn rejects_missing_directories_and_escaping_refs() {
        let base = std::env::temp_dir();
        for (git, common, symbolic) in [
            ("", ".git", "HEAD"),
            (".git", "", "HEAD"),
            (".git", ".git", "refs/../HEAD"),
            (".git", ".git", "/HEAD"),
        ] {
            assert!(resolve(&base, git, common, symbolic).is_none());
        }
    }
}
