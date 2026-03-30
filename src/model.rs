#[derive(Debug)]
pub enum LineKind {
    Added,
    Removed,
    Context,
}

#[derive(Debug)]
pub struct Line {
    pub kind: LineKind,
    pub content: String,
    pub old_num: Option<usize>,
    pub new_num: Option<usize>,
}

#[derive(Debug)]
pub struct Hunk {
    pub header: String,
    pub lines: Vec<Line>,
}

#[derive(Debug)]
pub enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed { from: String },
}

#[derive(Debug)]
pub struct DiffFile {
    pub path: String,
    pub status: FileStatus,
    pub hunks: Vec<Hunk>,
    pub added: usize,
    pub removed: usize,
    pub viewed: bool,
}

#[derive(Debug)]
pub struct Diff {
    pub files: Vec<DiffFile>,
}
