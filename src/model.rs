#[derive(Debug, Clone)]
pub enum LineKind {
    Added,
    Removed,
    Context,
}

#[derive(Debug, Clone)]
pub struct Line {
    pub kind: LineKind,
    pub content: String,
    pub old_num: Option<usize>,
    pub new_num: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct Hunk {
    pub header: String,
    pub lines: Vec<Line>,
}

#[derive(Debug, Clone)]
pub enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed { from: String },
}

#[derive(Debug, Clone)]
pub struct DiffFile {
    pub path: String,
    pub status: FileStatus,
    pub hunks: Vec<Hunk>,
    pub added: usize,
    pub removed: usize,
    pub viewed: bool,
}

#[derive(Debug, Clone)]
pub struct Diff {
    pub files: Vec<DiffFile>,
}

#[derive(Debug)]
pub struct CommitInfo {
    pub oid: String,
    pub short_oid: String,
    pub message: String,
    pub author: String,
    pub date: String,
}
