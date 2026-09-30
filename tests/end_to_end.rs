//! End-to-end checks over the full pipeline: git -> model -> tree -> render.
//!
//! The unit tests cover each layer in isolation with hand-built fixtures. These
//! drive a real repository through the same path the binary uses, so that a
//! mismatch between layers (a diff shape the tree drops, content the renderer
//! panics on) is caught even when every individual layer looks correct.

use std::fs;
use std::path::Path;

use git2::Repository;
use nit::{app, cache, git, model, tree, ui};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn init_repo(dir: &Path) -> Repository {
    let repo = Repository::init(dir).unwrap();
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "Test").unwrap();
    config.set_str("user.email", "test@test.com").unwrap();
    repo
}

fn commit_all(repo: &Repository, message: &str) -> git2::Oid {
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repo.find_tree(tree_id).unwrap();
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    let sig = repo.signature().unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
        .unwrap()
}

/// Render a diff in both view modes at several terminal sizes. Panics from the
/// renderer surface here.
fn render_all_modes(diff: &model::Diff) {
    let ss = syntect::parsing::SyntaxSet::load_defaults_newlines();
    let theme =
        syntect::highlighting::ThemeSet::load_defaults().themes["base16-ocean.dark"].clone();
    let colors = nit::config::ColorsConfig::default();
    let file_tree = tree::FileTree::from_files(&diff.files);
    let visible = file_tree.flatten(&Default::default());

    for (w, h) in [(120u16, 40u16), (80, 24), (40, 10), (20, 5), (6, 3)] {
        for mode in [app::DiffViewMode::Unified, app::DiffViewMode::SideBySide] {
            let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
            let mut a = app::App::new();
            a.view_mode = mode;
            terminal
                .draw(|frame| {
                    ui::draw(frame, &a, diff, &visible, "main", &ss, &theme, &colors);
                })
                .unwrap();
        }
    }
}

/// Every file the diff reports must be reachable in the sidebar tree, or the
/// user cannot navigate to changes the status bar is counting.
fn assert_tree_covers_diff(diff: &model::Diff) {
    let file_tree = tree::FileTree::from_files(&diff.files);
    let visible = file_tree.flatten(&Default::default());
    let mut found: Vec<usize> = visible
        .iter()
        .filter_map(|e| match &e.kind {
            tree::FlatEntryKind::File { file_index, .. } => Some(*file_index),
            _ => None,
        })
        .collect();
    found.sort_unstable();
    found.dedup();
    assert_eq!(
        found,
        (0..diff.files.len()).collect::<Vec<_>>(),
        "tree does not cover every file in the diff"
    );
}

#[test]
fn rename_non_ascii_and_binary_survive_the_full_pipeline() {
    let dir = tempfile::tempdir().unwrap();
    let repo = init_repo(dir.path());

    let body: String = (0..30).map(|i| format!("line {}\n", i)).collect();
    fs::write(dir.path().join("original.txt"), &body).unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/keep.rs"), "fn main() {}\n").unwrap();
    commit_all(&repo, "initial");

    // A rename, a non-ASCII file, a binary file, and a tab-indented source file.
    fs::rename(
        dir.path().join("original.txt"),
        dir.path().join("renamed.txt"),
    )
    .unwrap();
    fs::write(
        dir.path().join("nordisk.txt"),
        "blåbærsyltetøy\nLøsningen\n",
    )
    .unwrap();
    fs::write(dir.path().join("wide.txt"), "日本語のテスト\n").unwrap();
    fs::write(dir.path().join("bin.dat"), [0u8, 159, 146, 150, 0, 255]).unwrap();
    fs::write(
        dir.path().join("src/keep.rs"),
        "fn main() {\n\tlet x = 1;\n}\n",
    )
    .unwrap();
    let oid = commit_all(&repo, "rename, unicode, binary");

    let diff = git::get_commit_diff(&repo, &oid.to_string()).unwrap();

    // Rename detection must have collapsed the delete+add pair.
    let renamed = diff
        .files
        .iter()
        .find(|f| f.path == "renamed.txt")
        .expect("renamed file missing");
    match &renamed.status {
        model::FileStatus::Renamed { from } => assert_eq!(from, "original.txt"),
        other => panic!("expected Renamed, got {:?}", other),
    }
    assert!(
        !diff.files.iter().any(|f| f.path == "original.txt"),
        "rename should not also appear as a deletion"
    );

    assert_tree_covers_diff(&diff);
    render_all_modes(&diff);
}

#[test]
fn file_and_folder_name_collision_keeps_every_file() {
    let dir = tempfile::tempdir().unwrap();
    let repo = init_repo(dir.path());
    fs::write(dir.path().join("conflict"), "i am a file\n").unwrap();
    commit_all(&repo, "initial");

    // Replace the file `conflict` with a directory of the same name.
    fs::remove_file(dir.path().join("conflict")).unwrap();
    fs::create_dir(dir.path().join("conflict")).unwrap();
    fs::write(dir.path().join("conflict/inner.txt"), "now a folder\n").unwrap();
    let oid = commit_all(&repo, "file becomes folder");

    let diff = git::get_commit_diff(&repo, &oid.to_string()).unwrap();
    assert!(diff.files.len() >= 2, "expected both paths in the diff");
    assert_tree_covers_diff(&diff);
    render_all_modes(&diff);
}

#[test]
fn viewed_state_round_trips_through_the_index() {
    let dir = tempfile::tempdir().unwrap();
    let repo = init_repo(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "initial");
    fs::write(dir.path().join("a.txt"), "two\n").unwrap();

    let diff = git::get_uncommitted_diff(&repo).unwrap();
    assert_eq!(diff.files.len(), 1);

    // Marking viewed stages, unmarking unstages — the documented contract.
    git::stage_file(&repo, "a.txt", false).unwrap();
    assert!(git::get_staged_files(&repo).unwrap().contains("a.txt"));

    git::unstage_file(&repo, "a.txt").unwrap();
    assert!(!git::get_staged_files(&repo).unwrap().contains("a.txt"));
}

#[test]
fn selection_survives_a_file_appearing_earlier_in_the_diff() {
    let dir = tempfile::tempdir().unwrap();
    let repo = init_repo(dir.path());
    fs::write(dir.path().join("m.txt"), "one\n").unwrap();
    fs::write(dir.path().join("z.txt"), "one\n").unwrap();
    commit_all(&repo, "initial");

    fs::write(dir.path().join("z.txt"), "changed\n").unwrap();
    let before = git::get_uncommitted_diff(&repo).unwrap();
    let z_index = before.files.iter().position(|f| f.path == "z.txt").unwrap();

    let mut a = app::App::new();
    a.selected_file = z_index;
    let previous = a.selected_path(&before).map(str::to_owned);
    assert_eq!(previous.as_deref(), Some("z.txt"));

    // A new file sorts ahead of z.txt, shifting its positional index.
    fs::write(dir.path().join("a.txt"), "new\n").unwrap();
    let after = git::get_uncommitted_diff(&repo).unwrap();
    let file_tree = tree::FileTree::from_files(&after.files);
    let visible = file_tree.flatten(&Default::default());
    a.resync_selection(previous.as_deref(), &after, &visible);

    assert_eq!(
        after.files[a.selected_file].path, "z.txt",
        "selection must follow the file, not the index"
    );
}

#[test]
fn cache_row_counts_match_the_rendered_split_rows() {
    let dir = tempfile::tempdir().unwrap();
    let repo = init_repo(dir.path());
    let body: String = (0..40).map(|i| format!("line {}\n", i)).collect();
    fs::write(dir.path().join("a.txt"), &body).unwrap();
    commit_all(&repo, "initial");

    let changed: String = (0..40)
        .map(|i| {
            if i % 7 == 0 {
                format!("CHANGED {}\n", i)
            } else {
                format!("line {}\n", i)
            }
        })
        .collect();
    fs::write(dir.path().join("a.txt"), &changed).unwrap();
    let oid = commit_all(&repo, "edit");

    let diff = git::get_commit_diff(&repo, &oid.to_string()).unwrap();
    let dc = cache::DiffCache::new(&diff);

    for (i, file) in diff.files.iter().enumerate() {
        let rendered = nit::split::build_split_rows(&file.hunks).len();
        let cached = dc.diff_line_count(i, &diff, app::DiffViewMode::SideBySide);
        assert_eq!(
            cached, rendered,
            "cached split row count disagrees with the rows actually built for {}",
            file.path
        );
    }
}
