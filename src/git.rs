use anyhow::{Context, Result};
use git2::{Delta, DiffOptions, Repository};

use crate::model;

/// Open the git repository that contains the current directory.
pub fn open_repo() -> Result<Repository> {
    Repository::discover(".").context("not a git repository (or any parent)")
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
            let patch = git2::Patch::from_diff(&diff, delta_idx)
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
