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

pub trait RemoteProvider {
    fn fetch_diff(&self, pr_id: &str) -> Result<model::Diff>;

    fn fetch_comments(&self, pr_id: &str) -> Result<Vec<Comment>>;

    fn fetch_metadata(&self, pr_id: &str) -> Result<PrMetadata>;

    fn fetch_pr_list(&self, _limit: usize) -> Result<Vec<model::PrInfo>> {
        Ok(Vec::new())
    }

    fn fetch_authenticated_user(&self) -> Result<String> {
        anyhow::bail!("not supported by this provider")
    }
}
