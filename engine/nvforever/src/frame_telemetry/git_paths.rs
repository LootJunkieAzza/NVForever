//! Git metadata paths for build provenance (docs/design/frame-telemetry.md §2).

use std::path::{Component, Path, PathBuf};

pub struct GitPaths {
    pub head: PathBuf,
    pub branch: Option<PathBuf>,
    pub packed_refs: Vec<PathBuf>,
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
