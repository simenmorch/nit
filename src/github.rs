use std::process::Command;
use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use octocrab::Octocrab;
use octocrab::models::IssueState;
use octocrab::models::pulls::Comment as OctoComment;
use octocrab::models::pulls::PullRequest;
use octocrab::models::repos::{DiffEntry, DiffEntryStatus};
use octocrab::params;
use tokio::runtime::Runtime;

use crate::model;
use crate::provider::{self, RemoteProvider};

pub struct GitHubProvider {
    owner: String,
    repo: String,
    client: Octocrab,
    rt: Runtime,
}

impl GitHubProvider {
    pub fn new(owner: String, repo: String) -> Result<Self> {
        let token = resolve_token()?;

        let rt = Runtime::new().context("failed to create tokio runtime")?;

        // Octocrab uses tower internally, which requires a tokio runtime context
        let client = rt.block_on(async {
            Octocrab::builder()
                .personal_token(token)
                .build()
                .context("failed to build GitHub client")
        })?;

        Ok(Self {
            owner,
            repo,
            client,
            rt,
        })
    }
}

/// Try GITHUB_TOKEN env var, then fall back to `gh auth token`.
fn resolve_token() -> Result<String> {
    if let Ok(token) = std::env::var("GITHUB_TOKEN")
        && !token.is_empty()
    {
        return Ok(token);
    }

    let output = Command::new("gh")
        .args(["auth", "token"])
        .output()
        .context("failed to run `gh auth token` — is the GitHub CLI installed?")?;

    if output.status.success() {
        let token = String::from_utf8(output.stdout)
            .context("invalid UTF-8 from `gh auth token`")?
            .trim()
            .to_string();
        if !token.is_empty() {
            return Ok(token);
        }
    }

    bail!("no GitHub token found. Set GITHUB_TOKEN or run `gh auth login`")
}

impl RemoteProvider for GitHubProvider {
    fn fetch_authenticated_user(&self) -> Result<String> {
        self.rt.block_on(async {
            let user = self
                .client
                .current()
                .user()
                .await
                .context("failed to fetch authenticated user")?;
            Ok(user.login)
        })
    }

    fn fetch_pr_list(&self, limit: usize) -> Result<Vec<model::PrInfo>> {
        let pages: Vec<PullRequest> = self.rt.block_on(async {
            let page = self
                .client
                .pulls(&self.owner, &self.repo)
                .list()
                .state(params::State::All)
                .sort(params::pulls::Sort::Updated)
                .direction(params::Direction::Descending)
                .per_page(limit.min(100) as u8)
                .send()
                .await
                .context("failed to fetch pull requests")?;
            Ok::<_, anyhow::Error>(page.items)
        })?;

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let prs = pages
            .into_iter()
            .take(limit)
            .map(|pr| {
                let state = if pr.draft == Some(true) {
                    "draft"
                } else if pr.merged_at.is_some() {
                    "merged"
                } else {
                    match pr.state {
                        Some(IssueState::Closed) => "closed",
                        _ => "open",
                    }
                }
                .to_string();

                let updated_at = pr
                    .updated_at
                    .map(|dt| {
                        let secs = dt.timestamp();
                        crate::git::format_relative_time(now - secs)
                    })
                    .unwrap_or_else(|| "unknown".to_string());

                model::PrInfo {
                    number: pr.number,
                    title: pr.title.unwrap_or_default(),
                    author: pr.user.map(|u| u.login).unwrap_or_else(|| "unknown".into()),
                    state,
                    updated_at,
                }
            })
            .collect();

        Ok(prs)
    }

    fn fetch_diff(&self, pr_id: &str) -> Result<model::Diff> {
        let pr_number: u64 = pr_id.parse().context("PR id must be a number")?;

        let files: Vec<DiffEntry> = self.rt.block_on(async {
            let first_page = self
                .client
                .pulls(&self.owner, &self.repo)
                .list_files(pr_number)
                .await
                .context("failed to fetch PR files")?;
            self.client
                .all_pages(first_page)
                .await
                .context("failed to paginate PR files")
        })?;

        let mut diff_files: Vec<model::DiffFile> = Vec::new();

        for entry in files {
            let status = match entry.status {
                DiffEntryStatus::Added => model::FileStatus::Added,
                DiffEntryStatus::Removed => model::FileStatus::Deleted,
                DiffEntryStatus::Renamed => model::FileStatus::Renamed {
                    from: entry.previous_filename.unwrap_or_default(),
                },
                _ => model::FileStatus::Modified,
            };

            let hunks = match &entry.patch {
                Some(patch) => parse_patch(patch),
                None => Vec::new(),
            };
            let is_binary = entry.patch.is_none();

            let added = entry.additions as usize;
            let removed = entry.deletions as usize;

            diff_files.push(model::DiffFile {
                path: entry.filename,
                status,
                hunks,
                added,
                removed,
                viewed: false,
                is_binary,
            });
        }

        Ok(model::Diff { files: diff_files })
    }

    fn fetch_comments(&self, pr_id: &str) -> Result<Vec<provider::Comment>> {
        let pr_number: u64 = pr_id.parse().context("PR id must be a number")?;

        let comments: Vec<OctoComment> = self.rt.block_on(async {
            let first_page = self
                .client
                .pulls(&self.owner, &self.repo)
                .list_comments(Some(pr_number))
                .per_page(100)
                .send()
                .await
                .context("failed to fetch PR comments")?;
            self.client
                .all_pages(first_page)
                .await
                .context("failed to paginate PR comments")
        })?;

        let result = comments
            .into_iter()
            .map(|c| provider::Comment {
                author: c.user.map(|u| u.login).unwrap_or_else(|| "unknown".into()),
                body: c.body,
                created_at: c.created_at.format("%Y-%m-%d %H:%M").to_string(),
                path: c.path,
                line: c.line.map(|n| n as usize),
            })
            .collect();

        Ok(result)
    }

    fn fetch_metadata(&self, pr_id: &str) -> Result<provider::PrMetadata> {
        let pr_number: u64 = pr_id.parse().context("PR id must be a number")?;

        let pr = self.rt.block_on(async {
            self.client
                .pulls(&self.owner, &self.repo)
                .get(pr_number)
                .await
                .context("failed to fetch PR metadata")
        })?;

        let state = if pr.merged_at.is_some() {
            provider::PrState::Merged
        } else {
            match pr.state {
                Some(IssueState::Closed) => provider::PrState::Closed,
                _ => provider::PrState::Open,
            }
        };

        Ok(provider::PrMetadata {
            title: pr.title.unwrap_or_default(),
            author: pr.user.map(|u| u.login).unwrap_or_else(|| "unknown".into()),
            state,
            base_branch: pr.base.label.unwrap_or_default(),
            head_branch: pr.head.label.unwrap_or_default(),
        })
    }
}

/// Parse a GitHub patch string into our Hunk/Line model.
fn parse_patch(patch: &str) -> Vec<model::Hunk> {
    let mut hunks: Vec<model::Hunk> = Vec::new();
    let mut current_header = String::new();
    let mut current_lines: Vec<model::Line> = Vec::new();
    let mut old_num: usize = 0;
    let mut new_num: usize = 0;

    for text in patch.lines() {
        if text.starts_with("@@") {
            if !current_header.is_empty() {
                hunks.push(model::Hunk {
                    header: current_header,
                    lines: current_lines,
                });
            }
            // Reset unconditionally: anything accumulated before the first `@@`
            // is file-header preamble (`diff --git`, `index`, `---`, `+++`),
            // not hunk content.
            current_lines = Vec::new();
            current_header = text.to_string();

            if let Some((old_start, new_start)) = parse_hunk_header(text) {
                old_num = old_start;
                new_num = new_start;
            } else {
                old_num = 0;
                new_num = 0;
            }
            continue;
        }

        // Ignore everything until the first hunk header.
        if current_header.is_empty() {
            continue;
        }

        // "\ No newline at end of file" is a marker, not content. Treating it
        // as a context line would shift every following line number.
        if text.starts_with('\\') {
            continue;
        }

        if let Some(stripped) = text.strip_prefix('+') {
            current_lines.push(model::Line {
                kind: model::LineKind::Added,
                content: stripped.to_string(),
                old_num: None,
                new_num: Some(new_num),
            });
            new_num += 1;
        } else if let Some(stripped) = text.strip_prefix('-') {
            current_lines.push(model::Line {
                kind: model::LineKind::Removed,
                content: stripped.to_string(),
                old_num: Some(old_num),
                new_num: None,
            });
            old_num += 1;
        } else {
            let content = text.strip_prefix(' ').unwrap_or(text);
            current_lines.push(model::Line {
                kind: model::LineKind::Context,
                content: content.to_string(),
                old_num: Some(old_num),
                new_num: Some(new_num),
            });
            old_num += 1;
            new_num += 1;
        }
    }

    if !current_header.is_empty() {
        hunks.push(model::Hunk {
            header: current_header,
            lines: current_lines,
        });
    }

    hunks
}

fn parse_hunk_header(header: &str) -> Option<(usize, usize)> {
    let header = header.strip_prefix("@@ ")?;
    let parts: Vec<&str> = header.splitn(3, ' ').collect();
    if parts.len() < 2 {
        return None;
    }

    let old_start = parts[0]
        .strip_prefix('-')?
        .split(',')
        .next()?
        .parse::<usize>()
        .ok()?;

    let new_start = parts[1]
        .strip_prefix('+')?
        .split(',')
        .next()?
        .parse::<usize>()
        .ok()?;

    Some((old_start, new_start))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::LineKind;

    // ── parse_hunk_header ──

    #[test]
    fn hunk_header_standard() {
        assert_eq!(parse_hunk_header("@@ -10,5 +20,8 @@"), Some((10, 20)));
    }

    #[test]
    fn hunk_header_with_function_context() {
        assert_eq!(parse_hunk_header("@@ -1,3 +1,4 @@ fn main()"), Some((1, 1)));
    }

    #[test]
    fn hunk_header_no_comma() {
        assert_eq!(parse_hunk_header("@@ -1 +1 @@"), Some((1, 1)));
    }

    #[test]
    fn hunk_header_invalid() {
        assert_eq!(parse_hunk_header("not a header"), None);
    }

    #[test]
    fn hunk_header_empty() {
        assert_eq!(parse_hunk_header(""), None);
    }

    // ── parse_patch ──

    #[test]
    fn parse_patch_single_hunk() {
        let patch = "@@ -1,3 +1,4 @@\n context\n+added\n-removed";
        let hunks = parse_patch(patch);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].lines.len(), 3);
    }

    #[test]
    fn parse_patch_multi_hunk() {
        let patch = "@@ -1,2 +1,2 @@\n line1\n+line2\n@@ -10,2 +10,2 @@\n line3\n-line4";
        let hunks = parse_patch(patch);
        assert_eq!(hunks.len(), 2);
    }

    #[test]
    fn parse_patch_line_kinds() {
        let patch = "@@ -1,3 +1,3 @@\n context\n+added\n-removed";
        let hunks = parse_patch(patch);
        let lines = &hunks[0].lines;
        assert!(matches!(lines[0].kind, LineKind::Context));
        assert!(matches!(lines[1].kind, LineKind::Added));
        assert!(matches!(lines[2].kind, LineKind::Removed));
    }

    #[test]
    fn parse_patch_line_numbers() {
        let patch = "@@ -10,3 +20,3 @@\n context\n+added\n-removed";
        let hunks = parse_patch(patch);
        let lines = &hunks[0].lines;

        // Context line: both old and new
        assert_eq!(lines[0].old_num, Some(10));
        assert_eq!(lines[0].new_num, Some(20));

        // Added line: only new
        assert_eq!(lines[1].old_num, None);
        assert_eq!(lines[1].new_num, Some(21));

        // Removed line: only old
        assert_eq!(lines[2].old_num, Some(11));
        assert_eq!(lines[2].new_num, None);
    }

    #[test]
    fn parse_patch_content_strips_prefix() {
        let patch = "@@ -1,2 +1,2 @@\n context line\n+added line";
        let hunks = parse_patch(patch);
        assert_eq!(hunks[0].lines[0].content, "context line");
        assert_eq!(hunks[0].lines[1].content, "added line");
    }

    #[test]
    fn parse_patch_empty() {
        let hunks = parse_patch("");
        assert!(hunks.is_empty());
    }

    #[test]
    fn parse_patch_preserves_header() {
        let patch = "@@ -1,3 +1,4 @@ fn main()\n context";
        let hunks = parse_patch(patch);
        assert_eq!(hunks[0].header, "@@ -1,3 +1,4 @@ fn main()");
    }

    #[test]
    fn parse_patch_no_newline_marker_is_not_a_line() {
        // GitHub emits a literal "\ No newline at end of file" marker. It is not
        // content and must not consume a line number.
        let patch = "@@ -1,2 +1,2 @@\n one\n-two\n\\ No newline at end of file\n+two\n\\ No newline at end of file";
        let hunks = parse_patch(patch);
        let lines = &hunks[0].lines;

        assert_eq!(lines.len(), 3, "marker must not become a line");
        assert_eq!(lines[0].content, "one");
        assert_eq!(lines[1].content, "two");
        assert!(matches!(lines[1].kind, LineKind::Removed));
        assert_eq!(lines[2].content, "two");
        assert!(matches!(lines[2].kind, LineKind::Added));

        // Numbering must be unaffected by the markers.
        assert_eq!(lines[0].old_num, Some(1));
        assert_eq!(lines[0].new_num, Some(1));
        assert_eq!(lines[1].old_num, Some(2));
        assert_eq!(lines[2].new_num, Some(2));
    }

    #[test]
    fn parse_patch_no_newline_marker_midway_keeps_numbering() {
        let patch = "@@ -1,3 +1,3 @@\n-old\n\\ No newline at end of file\n+new\n ctx";
        let hunks = parse_patch(patch);
        let lines = &hunks[0].lines;

        assert_eq!(lines.len(), 3);
        // The context line after the marker must still be line 2/2.
        assert_eq!(lines[2].old_num, Some(2));
        assert_eq!(lines[2].new_num, Some(2));
    }

    #[test]
    fn parse_patch_ignores_preamble_before_first_hunk() {
        // Lines before the first @@ are file headers, not content.
        let patch =
            "diff --git a/x b/x\nindex abc..def 100644\n--- a/x\n+++ b/x\n@@ -1,1 +1,1 @@\n ctx";
        let hunks = parse_patch(patch);
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].lines.len(), 1, "preamble must not leak into hunk");
        assert_eq!(hunks[0].lines[0].content, "ctx");
    }

    #[test]
    fn parse_patch_crlf_content_is_preserved() {
        let patch = "@@ -1,1 +1,1 @@\r\n context\r\n";
        let hunks = parse_patch(patch);
        assert_eq!(hunks[0].lines[0].content, "context");
    }
}
