use anyhow::Result;

use crate::model;

#[derive(Debug)]
pub struct Comment {
    pub author: String,
    pub body: String,
    pub created_at: String,
    pub path: String,
    pub line: Option<usize>,
}

#[derive(Debug)]
pub enum PrState {
    Open,
    Merged,
    Closed,
}

#[derive(Debug)]
pub struct PrMetadata {
    pub title: String,
    pub author: String,
    pub state: PrState,
    pub base_branch: String,
    pub head_branch: String,
}

pub trait ReviewProvider {
    /// Fetch the diff for a pull/merge request.
    fn fetch_diff(&self, pr_id: &str) -> Result<model::Diff>;

    /// Fetch review comments attached to lines in the diff.
    fn fetch_comments(&self, pr_id: &str) -> Result<Vec<Comment>>;

    /// Fetch PR/MR metadata (title, author, state, branches).
    fn fetch_metadata(&self, pr_id: &str) -> Result<PrMetadata>;
}
