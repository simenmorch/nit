use std::process::Command;

use anyhow::{Context, Result, bail};
use octocrab::models::pulls::Comment as OctoComment;
use octocrab::models::repos::{DiffEntry, DiffEntryStatus};
use octocrab::models::IssueState;
use octocrab::Octocrab;
use tokio::runtime::Runtime;

use crate::model;
use crate::provider::{self, ReviewProvider};

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

        Ok(Self { owner, repo, client, rt })
    }
}

/// Try GITHUB_TOKEN env var, then fall back to `gh auth token`.
fn resolve_token() -> Result<String> {
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        if !token.is_empty() {
            return Ok(token);
        }
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

impl ReviewProvider for GitHubProvider {
    fn fetch_diff(&self, pr_id: &str) -> Result<model::Diff> {
        let pr_number: u64 = pr_id.parse().context("PR id must be a number")?;

        let files: Vec<DiffEntry> = self.rt.block_on(async {
            self.client
                .pulls(&self.owner, &self.repo)
                .list_files(pr_number)
                .await
                .context("failed to fetch PR files")
                .map(|page| page.items)
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

            let added = entry.additions as usize;
            let removed = entry.deletions as usize;

            diff_files.push(model::DiffFile {
                path: entry.filename,
                status,
                hunks,
                added,
                removed,
                viewed: false,
            });
        }

        Ok(model::Diff { files: diff_files })
    }

    fn fetch_comments(&self, pr_id: &str) -> Result<Vec<provider::Comment>> {
        let pr_number: u64 = pr_id.parse().context("PR id must be a number")?;

        let comments: Vec<OctoComment> = self.rt.block_on(async {
            self.client
                .pulls(&self.owner, &self.repo)
                .list_comments(Some(pr_number))
                .per_page(100)
                .send()
                .await
                .context("failed to fetch PR comments")
                .map(|page| page.items)
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
                current_lines = Vec::new();
            }

            current_header = text.to_string();

            if let Some((old_start, new_start)) = parse_hunk_header(text) {
                old_num = old_start;
                new_num = new_start;
            }
        } else if text.starts_with('+') {
            current_lines.push(model::Line {
                kind: model::LineKind::Added,
                content: text[1..].to_string(),
                old_num: None,
                new_num: Some(new_num),
            });
            new_num += 1;
        } else if text.starts_with('-') {
            current_lines.push(model::Line {
                kind: model::LineKind::Removed,
                content: text[1..].to_string(),
                old_num: Some(old_num),
                new_num: None,
            });
            old_num += 1;
        } else {
            let content = if text.starts_with(' ') { &text[1..] } else { text };
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
