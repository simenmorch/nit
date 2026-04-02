use anyhow::{Context, Result};
use git2::{Delta, DiffOptions, Repository};

use crate::model;

/// Open the git repository that contains the current directory.
pub fn open_repo() -> Result<Repository> {
    Repository::discover(".").context("not a git repository (or any parent)")
}

/// Parse owner/repo from the origin remote URL.
/// Supports SSH (git@github.com:owner/repo.git) and HTTPS (https://github.com/owner/repo.git).
pub fn owner_repo_from_remote(repo: &Repository) -> Result<(String, String)> {
    let remote = repo
        .find_remote("origin")
        .context("no 'origin' remote found")?;

    let url = remote.url().context("remote URL is not valid UTF-8")?;

    parse_remote_url(url)
}

fn parse_remote_url(url: &str) -> Result<(String, String)> {
    // SSH: git@github.com:owner/repo.git
    if let Some(path) = url.strip_prefix("git@").and_then(|s| s.split(':').nth(1)) {
        let path = path.strip_suffix(".git").unwrap_or(path);
        let parts: Vec<&str> = path.splitn(2, '/').collect();

        if parts.len() == 2 {
            return Ok((parts[0].to_string(), parts[1].to_string()));
        }
    }

    // HTTPS: https://github.com/owner/repo.git
    if url.starts_with("https://") || url.starts_with("http://") {
        let path = url
            .split("://")
            .nth(1)
            .and_then(|s| s.splitn(2, '/').nth(1))
            .unwrap_or("");

        let path = path.strip_suffix(".git").unwrap_or(path);
        let test = path.strip_suffix(".git").unwrap_or(path);
        let parts: Vec<&str> = path.splitn(2, '/').collect();
        if parts.len() == 2 {
            return Ok((parts[0].to_string(), parts[1].to_string()));
        }
    }

    anyhow::bail!("could not parse owner/repo from remote URL: {}", url)
}

/// Get the current branch name, or "HEAD" if detached.
pub fn branch_name(repo: &Repository) -> String {
    repo.head()
        .ok()
        .and_then(|h| h.shorthand().map(String::from))
        .unwrap_or_else(|| "HEAD".to_string())
}

/// Get the HEAD commit's tree, or None if the repo has no commits yet.
fn head_tree(repo: &Repository) -> Result<Option<git2::Tree<'_>>> {
    match repo.head() {
        Ok(head) => {
            let tree = head
                .peel_to_tree()
                .context("failed to get tree from HEAD")?;
            Ok(Some(tree))
        }
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => Ok(None),
        Err(e) => Err(e).context("failed to read HEAD"),
    }
}

/// Read all uncommitted changes (staged + unstaged) and return our Diff model.
pub fn get_uncommitted_diff(repo: &Repository) -> Result<model::Diff> {
    let tree = head_tree(repo)?;

    let mut opts = DiffOptions::new();
    opts.context_lines(5);

    let diff = repo
        .diff_tree_to_workdir_with_index(tree.as_ref(), Some(&mut opts))
        .context("failed to compute diff")?;

    build_diff(&diff)
}

/// Diff a single commit against its parent.
pub fn get_commit_diff(repo: &Repository, rev: &str) -> Result<model::Diff> {
    let obj = repo
        .revparse_single(rev)
        .with_context(|| format!("could not resolve '{}'", rev))?;

    let commit = obj
        .peel_to_commit()
        .with_context(|| format!("'{}' is not a commit", rev))?;

    let new_tree = commit.tree().context("failed to get commit tree")?;

    // Get parent tree (None for root commit = diff against empty tree)
    let parent_tree = if commit.parent_count() > 0 {
        Some(commit.parent(0)?.tree()?)
    } else {
        None
    };

    let mut opts = DiffOptions::new();
    opts.context_lines(5);

    let diff = repo
        .diff_tree_to_tree(parent_tree.as_ref(), Some(&new_tree), Some(&mut opts))
        .context("failed to compute commit diff")?;

    build_diff(&diff)
}

/// Diff between two revisions.
pub fn get_range_diff(repo: &Repository, from: &str, to: &str) -> Result<model::Diff> {
    let from_obj = repo
        .revparse_single(from)
        .with_context(|| format!("could not resolve '{}'", from))?;
    let to_obj = repo
        .revparse_single(to)
        .with_context(|| format!("could not resolve '{}'", to))?;

    let from_tree = from_obj
        .peel_to_tree()
        .with_context(|| format!("'{}' does not point to a tree", from))?;
    let to_tree = to_obj
        .peel_to_tree()
        .with_context(|| format!("'{}' does not point to a tree", to))?;

    let mut opts = DiffOptions::new();
    opts.context_lines(5);

    let diff = repo
        .diff_tree_to_tree(Some(&from_tree), Some(&to_tree), Some(&mut opts))
        .context("failed to compute range diff")?;

    build_diff(&diff)
}

/// Convert a git2 Diff into our model.
fn build_diff(diff: &git2::Diff) -> Result<model::Diff> {
    let mut files: Vec<model::DiffFile> = Vec::new();

    for delta_idx in 0..diff.deltas().len() {
        let delta = diff.deltas().nth(delta_idx).unwrap();

        let path = delta
            .new_file()
            .path()
            .or(delta.old_file().path())
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "(unknown)".to_string());

        let status = match delta.status() {
            Delta::Added | Delta::Untracked => model::FileStatus::Added,
            Delta::Deleted => model::FileStatus::Deleted,
            Delta::Renamed => model::FileStatus::Renamed {
                from: delta
                    .old_file()
                    .path()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            },
            _ => model::FileStatus::Modified,
        };

        let is_binary = delta.flags().is_binary();

        let mut hunks: Vec<model::Hunk> = Vec::new();
        let mut added: usize = 0;
        let mut removed: usize = 0;

        if !is_binary {
            let patch = git2::Patch::from_diff(diff, delta_idx)
                .context("failed to get patch")?;

            if let Some(patch) = patch {
                for hunk_idx in 0..patch.num_hunks() {
                    let (hunk, _num_lines) = patch.hunk(hunk_idx)?;

                    let header = String::from_utf8_lossy(hunk.header()).into_owned();

                    let mut lines: Vec<model::Line> = Vec::new();

                    for line_idx in 0..patch.num_lines_in_hunk(hunk_idx)? {
                        let line = patch.line_in_hunk(hunk_idx, line_idx)?;

                        let kind = match line.origin() {
                            '+' => {
                                added += 1;
                                model::LineKind::Added
                            }
                            '-' => {
                                removed += 1;
                                model::LineKind::Removed
                            }
                            _ => model::LineKind::Context,
                        };

                        let content =
                            String::from_utf8_lossy(line.content()).into_owned();

                        lines.push(model::Line {
                            kind,
                            content,
                            old_num: line.old_lineno().map(|n| n as usize),
                            new_num: line.new_lineno().map(|n| n as usize),
                        });
                    }

                    hunks.push(model::Hunk { header, lines });
                }
            }
        }

        files.push(model::DiffFile {
            path,
            status,
            hunks,
            added,
            removed,
            viewed: false,
        });
    }

    Ok(model::Diff { files })
}
