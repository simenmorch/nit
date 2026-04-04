use std::path::Path;
use std::time::SystemTime;

use anyhow::{Context, Result};
use git2::{Delta, DiffOptions, Repository, Sort};

use crate::model;

/// Open the git repository that contains the current directory.
pub fn open_repo() -> Result<Repository> {
    Repository::discover(".").context("not a git repository (or any parent)")
}

/// Stage a file in the git index. For deleted files, removes the entry.
pub fn stage_file(repo: &Repository, path: &str, deleted: bool) -> Result<()> {
    let mut index = repo.index().context("failed to open index")?;
    if deleted {
        index.remove_path(Path::new(path)).context("failed to remove path from index")?;
    } else {
        index.add_path(Path::new(path)).context("failed to add path to index")?;
    }
    index.write().context("failed to write index")?;
    Ok(())
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
            .and_then(|s| s.split_once('/').map(|x| x.1))
            .unwrap_or("");

        let path = path.strip_suffix(".git").unwrap_or(path);
        let parts: Vec<&str> = path.splitn(2, '/').collect();
        if parts.len() == 2 {
            return Ok((parts[0].to_string(), parts[1].to_string()));
        }
    }

    // Strip credentials from URL before including in error message
    let safe_url = if let Some(rest) = url.split_once("://").map(|(scheme, rest)| {
        if let Some((_, after_at)) = rest.split_once('@') {
            format!("{}://{}", scheme, after_at)
        } else {
            url.to_string()
        }
    }) {
        rest
    } else {
        url.to_string()
    };
    anyhow::bail!("could not parse owner/repo from remote URL: {}", safe_url)
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

/// List recent commits from HEAD.
pub fn get_commit_log(repo: &Repository, limit: usize) -> Result<Vec<model::CommitInfo>> {
    let mut revwalk = repo.revwalk().context("failed to create revwalk")?;
    revwalk.push_head().context("failed to push HEAD to revwalk")?;
    revwalk.set_sorting(Sort::TIME | Sort::TOPOLOGICAL)?;

    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let mut commits = Vec::new();
    for oid_result in revwalk.take(limit) {
        let oid = oid_result.context("revwalk error")?;
        let commit = repo.find_commit(oid).context("failed to find commit")?;

        let message = commit
            .summary()
            .unwrap_or("(no message)")
            .to_string();
        let author = commit
            .author()
            .name()
            .unwrap_or("(unknown)")
            .to_string();
        let date = format_relative_time(now - commit.time().seconds());

        let full_oid = oid.to_string();
        let short_oid = full_oid[..7].to_string();
        commits.push(model::CommitInfo {
            oid: full_oid,
            short_oid,
            message,
            author,
            date,
        });
    }
    Ok(commits)
}

pub fn format_relative_time(seconds_ago: i64) -> String {
    const MINUTE: i64 = 60;
    const TWO_MINUTES: i64 = 2 * MINUTE;
    const HOUR: i64 = 60 * MINUTE;
    const TWO_HOURS: i64 = 2 * HOUR;
    const DAY: i64 = 24 * HOUR;
    const TWO_DAYS: i64 = 2 * DAY;
    const WEEK: i64 = 7 * DAY;
    const TWO_WEEKS: i64 = 2 * WEEK;
    const MONTH: i64 = 30 * DAY;
    const TWO_MONTHS: i64 = 2 * MONTH;
    const YEAR: i64 = 365 * DAY;
    const TWO_YEARS: i64 = 2 * YEAR;

    match seconds_ago {
        ..0 => "in the future".to_string(),
        0..MINUTE => "just now".to_string(),
        MINUTE..TWO_MINUTES => "1 minute ago".to_string(),
        s @ TWO_MINUTES..HOUR => format!("{} minutes ago", s / MINUTE),
        HOUR..TWO_HOURS => "1 hour ago".to_string(),
        s @ TWO_HOURS..DAY => format!("{} hours ago", s / HOUR),
        DAY..TWO_DAYS => "1 day ago".to_string(),
        s @ TWO_DAYS..WEEK => format!("{} days ago", s / DAY),
        WEEK..TWO_WEEKS => "1 week ago".to_string(),
        s @ TWO_WEEKS..MONTH => format!("{} weeks ago", s / WEEK),
        MONTH..TWO_MONTHS => "1 month ago".to_string(),
        s @ TWO_MONTHS..YEAR => format!("{} months ago", s / MONTH),
        YEAR..TWO_YEARS => "1 year ago".to_string(),
        s => format!("{} years ago", s / YEAR),
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    // ── parse_remote_url ──

    #[test]
    fn parse_ssh_remote() {
        let (owner, repo) = parse_remote_url("git@github.com:owner/repo.git").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn parse_ssh_remote_no_dotgit() {
        let (owner, repo) = parse_remote_url("git@github.com:owner/repo").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn parse_https_remote() {
        let (owner, repo) = parse_remote_url("https://github.com/owner/repo.git").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn parse_https_remote_no_dotgit() {
        let (owner, repo) = parse_remote_url("https://github.com/owner/repo").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn parse_invalid_remote() {
        assert!(parse_remote_url("not-a-url").is_err());
    }

    // ── Temp repo helpers ──

    fn init_repo(dir: &Path) -> Repository {
        let repo = Repository::init(dir).unwrap();
        // Configure a dummy user for commits
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test").unwrap();
        config.set_str("user.email", "test@test.com").unwrap();
        repo
    }

    fn commit_file(repo: &Repository, path: &str, content: &str, message: &str) -> git2::Oid {
        let dir = repo.workdir().unwrap();
        let file_path = dir.join(path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&file_path, content).unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(Path::new(path)).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();

        let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();
        let sig = repo.signature().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .unwrap()
    }

    // ── Diff with temp repos ──

    #[test]
    fn uncommitted_diff_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        // Create initial commit so HEAD exists
        commit_file(&repo, "init.txt", "init", "initial commit");
        // Stage a new file without committing
        fs::write(dir.path().join("new.txt"), "hello").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("new.txt")).unwrap();
        index.write().unwrap();

        let diff = get_uncommitted_diff(&repo).unwrap();
        assert_eq!(diff.files.len(), 1);
        assert_eq!(diff.files[0].path, "new.txt");
        assert!(matches!(diff.files[0].status, model::FileStatus::Added));
    }

    #[test]
    fn uncommitted_diff_modified() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        commit_file(&repo, "file.txt", "original", "initial");
        fs::write(dir.path().join("file.txt"), "modified").unwrap();

        let diff = get_uncommitted_diff(&repo).unwrap();
        assert_eq!(diff.files.len(), 1);
        assert!(matches!(diff.files[0].status, model::FileStatus::Modified));
    }

    #[test]
    fn commit_diff_single() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        commit_file(&repo, "a.txt", "first", "first commit");
        let oid = commit_file(&repo, "a.txt", "second", "second commit");

        let diff = get_commit_diff(&repo, &oid.to_string()).unwrap();
        assert_eq!(diff.files.len(), 1);
        assert_eq!(diff.files[0].path, "a.txt");
        assert!(diff.files[0].added > 0 || diff.files[0].removed > 0);
    }

    #[test]
    fn range_diff() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        let from = commit_file(&repo, "a.txt", "v1", "commit 1");
        commit_file(&repo, "a.txt", "v2", "commit 2");
        let to = commit_file(&repo, "b.txt", "new", "commit 3");

        let diff = get_range_diff(&repo, &from.to_string(), &to.to_string()).unwrap();
        // Should show changes between commit 1 and commit 3
        assert!(!diff.files.is_empty());
    }

    #[test]
    fn root_commit_diff() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        let oid = commit_file(&repo, "first.txt", "content", "root commit");

        let diff = get_commit_diff(&repo, &oid.to_string()).unwrap();
        assert_eq!(diff.files.len(), 1);
        assert!(matches!(diff.files[0].status, model::FileStatus::Added));
    }

    // ── Commit log ──

    #[test]
    fn commit_log_returns_commits_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        commit_file(&repo, "a.txt", "v1", "first commit");
        commit_file(&repo, "a.txt", "v2", "second commit");
        commit_file(&repo, "a.txt", "v3", "third commit");

        let log = get_commit_log(&repo, 100).unwrap();
        assert_eq!(log.len(), 3);
        assert_eq!(log[0].message, "third commit");
        assert_eq!(log[1].message, "second commit");
        assert_eq!(log[2].message, "first commit");
    }

    #[test]
    fn commit_log_respects_limit() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        commit_file(&repo, "a.txt", "v1", "first");
        commit_file(&repo, "a.txt", "v2", "second");
        commit_file(&repo, "a.txt", "v3", "third");

        let log = get_commit_log(&repo, 2).unwrap();
        assert_eq!(log.len(), 2);
    }

    #[test]
    fn commit_log_has_short_oid() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        commit_file(&repo, "a.txt", "v1", "initial");

        let log = get_commit_log(&repo, 10).unwrap();
        assert_eq!(log[0].oid.len(), 40);
        assert_eq!(log[0].short_oid.len(), 7);
    }

    #[test]
    fn commit_log_captures_author() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        commit_file(&repo, "a.txt", "v1", "initial");

        let log = get_commit_log(&repo, 10).unwrap();
        assert_eq!(log[0].author, "Test");
    }

    // ── format_relative_time ──

    #[test]
    fn relative_time_just_now() {
        assert_eq!(format_relative_time(30), "just now");
    }

    #[test]
    fn relative_time_minutes() {
        assert_eq!(format_relative_time(120), "2 minutes ago");
    }

    #[test]
    fn relative_time_hours() {
        assert_eq!(format_relative_time(7200), "2 hours ago");
    }

    #[test]
    fn relative_time_days() {
        assert_eq!(format_relative_time(172800), "2 days ago");
    }

    #[test]
    fn relative_time_future() {
        assert_eq!(format_relative_time(-10), "in the future");
    }

    #[test]
    fn empty_repo_uncommitted_staged() {
        let dir = tempfile::tempdir().unwrap();
        let repo = init_repo(dir.path());
        // No commits yet, but stage a file
        fs::write(dir.path().join("new.txt"), "hello").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("new.txt")).unwrap();
        index.write().unwrap();

        let diff = get_uncommitted_diff(&repo).unwrap();
        assert_eq!(diff.files.len(), 1);
    }
}
