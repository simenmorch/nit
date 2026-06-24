# Sidebar Polish Features Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Layer power-user polish on top of the sidebar overview improvements: show the rename source for renamed files, add a mini churn bar, make the sidebar width keyboard-resizable, add a "jump to next unviewed" shortcut, render the file status as a colored cell badge, and let the user cycle through sort orders.

**Architecture:** Splits between pure helpers in `app.rs` / `ui.rs` (testable) and keymap wiring in `main.rs`. Two persistent state fields are added to `App`: `sidebar_width: u16` (Task 3) and `sort_order: SortOrder` (Task 6). `FileTree::flatten` learns a sort parameter (Task 6) that re-orders sibling entries before emission.

**Tech Stack:** Rust, ratatui, crossterm key events.

**Prerequisite:** This plan assumes the seven tasks from `2026-06-09-sidebar-overview-improvements.md` are already merged. In particular Tasks 2 (right-aligned stats), 3 (full-row selection background), 5 (folder counts), and 7 (tree connectors) are referenced by file-position context.

**Order rationale:** Cosmetic, self-contained features first (1, 2, 5). Then state/keymap additions (3, 4). Sort order (6) last because it touches `FileTree::flatten` and re-runs every render.

---

### Task 1: Render rename source on the selected file

`model::FileStatus::Renamed { from }` carries the old path but the sidebar drops it (`ui.rs:181`). When the *selected* row is a renamed file, show `from → name` on the row (or as a second muted line beneath). We render inline for simplicity.

**Files:**
- Modify: `src/ui.rs::draw_sidebar` (file arm)
- Test: `src/ui.rs` tests module — extract the label builder

- [ ] **Step 1: Write the failing test**

Append to `src/ui.rs` tests:

```rust
#[test]
fn rename_label_shows_arrow_when_renamed() {
    let f = DiffFile {
        path: "new/path.rs".to_string(),
        status: FileStatus::Renamed { from: "old/path.rs".to_string() },
        hunks: vec![],
        added: 0,
        removed: 0,
        viewed: false,
    };
    assert_eq!(crate::ui::file_display_label(&f, "path.rs"), "old/path.rs → path.rs");
}

#[test]
fn rename_label_returns_name_for_non_renamed() {
    let f = DiffFile {
        path: "x.rs".to_string(),
        status: FileStatus::Modified,
        hunks: vec![],
        added: 0,
        removed: 0,
        viewed: false,
    };
    assert_eq!(crate::ui::file_display_label(&f, "x.rs"), "x.rs");
}
```

(Imports needed: `use crate::model::{DiffFile, FileStatus};`.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib file_display_label`
Expected: FAIL — function not found.

- [ ] **Step 3: Implement the helper**

In `src/ui.rs` near `folder_file_counts`:

```rust
pub(crate) fn file_display_label(file: &model::DiffFile, name: &str) -> String {
    match &file.status {
        model::FileStatus::Renamed { from } => format!("{} → {}", from, name),
        _ => name.to_string(),
    }
}
```

In the file arm of `draw_sidebar`, replace the existing `Span::styled(name.clone(), name_style)` with:

```rust
let label = file_display_label(file, name);
Span::styled(label, name_style)
```

- [ ] **Step 4: Verify**

Run: `cargo test --lib && cargo clippy`
Expected: pass.

Run: `cargo run` against a branch with a renamed file (`git mv` something for a quick test).
Expected: renamed file shows `old/path.rs → new.rs` in the sidebar.

- [ ] **Step 5: Commit**

```bash
git add src/ui.rs
git commit -m "Show rename source in sidebar for renamed files"
```

---

### Task 2: Mini churn bar

A short block-glyph bar (e.g. `▆▆▆▂▂`) per file gives an instant visual sense of churn weight. Five cells, proportionally filled green/red based on `added`/`removed` over the file's max.

**Files:**
- Modify: `src/ui.rs` (new helper + use in sidebar file arm)

- [ ] **Step 1: Write the failing test**

Append to `src/ui.rs` tests module:

```rust
#[test]
fn churn_bar_all_added() {
    let bar = crate::ui::churn_bar(10, 0, 5);
    // 5 green blocks
    assert_eq!(bar.len(), 5);
    assert!(bar.iter().all(|(_, kind)| *kind == crate::ui::ChurnCell::Added));
}

#[test]
fn churn_bar_half_added_half_removed() {
    let bar = crate::ui::churn_bar(5, 5, 4);
    // 2 added, 2 removed (or 2-2 split for 4 cells)
    let added = bar.iter().filter(|(_, k)| *k == crate::ui::ChurnCell::Added).count();
    let removed = bar.iter().filter(|(_, k)| *k == crate::ui::ChurnCell::Removed).count();
    assert_eq!(added, 2);
    assert_eq!(removed, 2);
}

#[test]
fn churn_bar_pad_with_empty_when_small() {
    // 1 added line into a 5-cell bar → 1 added cell, 4 empty
    let bar = crate::ui::churn_bar(1, 0, 5);
    let empty = bar.iter().filter(|(_, k)| *k == crate::ui::ChurnCell::Empty).count();
    assert_eq!(empty, 4);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib churn_bar`
Expected: FAIL — items not found.

- [ ] **Step 3: Implement the helper**

In `src/ui.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChurnCell {
    Added,
    Removed,
    Empty,
}

/// Produce `cells` cells of (`▆`, ChurnCell) representing the add/remove ratio.
/// `total_max` is the largest add+remove across all files in the diff; if zero,
/// returns all-empty.
pub(crate) fn churn_bar(added: usize, removed: usize, cells: usize) -> Vec<(char, ChurnCell)> {
    let glyph = '▆';
    let total = added + removed;
    if total == 0 || cells == 0 {
        return (0..cells).map(|_| (glyph, ChurnCell::Empty)).collect();
    }
    // How many of `cells` are filled at all (vs empty), proportionally to total
    // up to some cap. For sidebar use, cap at `cells` to avoid scale dependency.
    let filled = total.min(cells);
    let added_cells = if total == 0 { 0 } else { (added * filled).div_ceil(total) };
    let removed_cells = filled - added_cells;
    let empty = cells - filled;

    let mut out = Vec::with_capacity(cells);
    for _ in 0..added_cells { out.push((glyph, ChurnCell::Added)); }
    for _ in 0..removed_cells { out.push((glyph, ChurnCell::Removed)); }
    for _ in 0..empty { out.push((glyph, ChurnCell::Empty)); }
    out
}
```

- [ ] **Step 4: Run the helper tests**

Run: `cargo test --lib churn_bar`
Expected: pass.

- [ ] **Step 5: Render the bar in the sidebar**

In `src/ui.rs::draw_sidebar` file arm, build the bar spans and insert them between the filename and the stats (before the right-alignment padding):

```rust
let bar = churn_bar(file.added, file.removed, 5);
let mut bar_spans: Vec<Span> = Vec::with_capacity(5);
for (ch, kind) in bar {
    let color = match kind {
        ChurnCell::Added => colors.fg_added,
        ChurnCell::Removed => colors.fg_removed,
        ChurnCell::Empty => colors.fg_muted,
    };
    bar_spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
}
```

Append `bar_spans` to your `prefix_spans` vec **before** computing `prefix_width` for the right-aligned stats. Add 1 char of padding between filename and bar for breathing room.

- [ ] **Step 6: Verify**

Run: `cargo test --lib && cargo clippy`
Expected: pass.

Run: `cargo run`
Expected: every file row shows a 5-cell coloured bar between the filename and the stats column.

- [ ] **Step 7: Commit**

```bash
git add src/ui.rs
git commit -m "Render mini churn bar per file in sidebar"
```

---

### Task 3: Resizable sidebar width

`SIDEBAR_WIDTH` is currently a hardcoded `40` (`ui.rs:18`). Make it stateful on `App` and bind `<` / `>` to shrink/grow by 4 columns (clamped 20..=120).

**Files:**
- Modify: `src/app.rs` (add `sidebar_width: u16`, methods, tests)
- Modify: `src/ui.rs` (use `app.sidebar_width` instead of the constant)
- Modify: `src/main.rs` (key bindings)

- [ ] **Step 1: Write failing tests**

Append to `src/app.rs` tests module:

```rust
#[test]
fn shrink_sidebar_clamps_at_min() {
    let mut app = App::new();
    app.sidebar_width = 22;
    app.shrink_sidebar();
    assert_eq!(app.sidebar_width, 20);
    app.shrink_sidebar();
    assert_eq!(app.sidebar_width, 20); // clamped
}

#[test]
fn grow_sidebar_clamps_at_max() {
    let mut app = App::new();
    app.sidebar_width = 118;
    app.grow_sidebar();
    assert_eq!(app.sidebar_width, 120);
    app.grow_sidebar();
    assert_eq!(app.sidebar_width, 120); // clamped
}

#[test]
fn sidebar_width_default_matches_legacy() {
    let app = App::new();
    assert_eq!(app.sidebar_width, 40);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib sidebar_width shrink_sidebar grow_sidebar`
Expected: FAIL — fields/methods not found.

- [ ] **Step 3: Add the field and methods to `App`**

In `src/app.rs` add to the `App` struct (around `ui.rs:155-188`):

```rust
pub sidebar_width: u16,
```

In `App::new()`:

```rust
sidebar_width: 40,
```

Add methods:

```rust
pub const SIDEBAR_MIN: u16 = 20;
pub const SIDEBAR_MAX: u16 = 120;
pub const SIDEBAR_STEP: u16 = 4;

pub fn shrink_sidebar(&mut self) {
    self.sidebar_width = self.sidebar_width.saturating_sub(Self::SIDEBAR_STEP).max(Self::SIDEBAR_MIN);
}

pub fn grow_sidebar(&mut self) {
    self.sidebar_width = (self.sidebar_width + Self::SIDEBAR_STEP).min(Self::SIDEBAR_MAX);
}
```

(The `const`s can live as `pub const` items in the impl block.)

- [ ] **Step 4: Wire `SIDEBAR_WIDTH` constant out**

In `src/ui.rs:18` delete the `const SIDEBAR_WIDTH: u16 = 40;`. In `draw` (`ui.rs:74`) replace `SIDEBAR_WIDTH` with `app.sidebar_width`.

- [ ] **Step 5: Bind keys in `main.rs`**

Grep for the existing key-handling block for sidebar focus (e.g. `KeyCode::Char('h')`). Add new arms (only when the diff tab is active and focus is on sidebar — match existing pattern):

```rust
KeyCode::Char('<') => app.shrink_sidebar(),
KeyCode::Char('>') => app.grow_sidebar(),
```

Find the right location by running:
```bash
rg 'toggle_sidebar' src/main.rs
```

The new arms belong next to `toggle_sidebar()` handling.

- [ ] **Step 6: Update help modal**

In `src/ui.rs::draw_help_modal` (`ui.rs:981-1063`), under the **Diff — Sidebar** section, add:

```rust
("< / >", "Shrink / grow sidebar width"),
```

- [ ] **Step 7: Verify**

Run: `cargo test --lib && cargo clippy`
Expected: pass.

Run: `cargo run`
Expected: pressing `>` widens the sidebar by 4 columns, `<` narrows it. Clamps at 20 and 120.

- [ ] **Step 8: Commit**

```bash
git add src/app.rs src/ui.rs src/main.rs
git commit -m "Resizable sidebar with < / > keys"
```

---

### Task 4: Jump to next unviewed file

Reviewing a 30-file PR with some files already viewed, you want to skip directly to the next unreviewed file. Bind `U` (capital) to advance the sidebar selection to the next file with `viewed == false`, wrapping at the end (or stopping — pick one; we wrap to feel like `n` for search).

**Files:**
- Modify: `src/app.rs` (new method + tests)
- Modify: `src/main.rs` (key binding)
- Modify: `src/ui.rs::draw_help_modal` (help entry)

- [ ] **Step 1: Write failing tests**

Append to `src/app.rs` tests module:

```rust
#[test]
fn jump_to_next_unviewed_advances() {
    let mut diff = make_diff(vec![
        make_file("a.rs", vec![make_hunk("@@", vec![])]),
        make_file("b.rs", vec![make_hunk("@@", vec![])]),
        make_file("c.rs", vec![make_hunk("@@", vec![])]),
    ]);
    diff.files[1].viewed = false; // explicit
    diff.files[0].viewed = true;
    let visible = flat_entries_for(&diff);
    let mut app = App::new();
    app.selected = 0;
    app.selected_file = 0;
    app.jump_to_next_unviewed(&diff, &visible);
    assert_eq!(app.selected_file, 1); // b.rs (next unviewed)
}

#[test]
fn jump_to_next_unviewed_wraps_when_no_later_match() {
    let mut diff = make_diff(vec![
        make_file("a.rs", vec![]),
        make_file("b.rs", vec![]),
    ]);
    diff.files[0].viewed = false;
    diff.files[1].viewed = true;
    let visible = flat_entries_for(&diff);
    let mut app = App::new();
    app.selected_file = 1;
    // find entry index for b.rs
    for (i, e) in visible.iter().enumerate() {
        if let FlatEntryKind::File { file_index: 1, .. } = e.kind {
            app.selected = i;
        }
    }
    app.jump_to_next_unviewed(&diff, &visible);
    assert_eq!(app.selected_file, 0); // wraps back to a.rs
}

#[test]
fn jump_to_next_unviewed_noop_when_all_viewed() {
    let mut diff = make_diff(vec![make_file("a.rs", vec![]), make_file("b.rs", vec![])]);
    diff.files[0].viewed = true;
    diff.files[1].viewed = true;
    let visible = flat_entries_for(&diff);
    let mut app = App::new();
    app.selected_file = 0;
    app.jump_to_next_unviewed(&diff, &visible);
    // selection unchanged
    assert_eq!(app.selected_file, 0);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib jump_to_next_unviewed`
Expected: FAIL — method not found.

- [ ] **Step 3: Implement the method**

In `src/app.rs`'s `App` impl block, near `select_next_file` (`app.rs:265-285`):

```rust
/// Move sidebar selection to the next File entry whose backing DiffFile
/// has `viewed == false`. Wraps around if there is no later candidate.
/// No-op if every file is viewed.
pub fn jump_to_next_unviewed(
    &mut self,
    diff: &crate::model::Diff,
    visible: &[FlatEntry],
) {
    let len = visible.len();
    if len == 0 { return; }
    // Search forward from selected+1, then wrap.
    for offset in 1..=len {
        let i = (self.selected + offset) % len;
        if let FlatEntryKind::File { file_index, .. } = &visible[i].kind
            && !diff.files[*file_index].viewed
        {
            self.selected = i;
            self.selected_file = *file_index;
            self.scroll = 0;
            return;
        }
    }
}
```

- [ ] **Step 4: Verify tests pass**

Run: `cargo test --lib jump_to_next_unviewed`
Expected: pass.

- [ ] **Step 5: Wire the keybinding**

In `src/main.rs`, in the diff-tab sidebar key handling, add (near `mark_viewed_and_next`):

```rust
KeyCode::Char('U') => app.jump_to_next_unviewed(diff, visible),
```

Pass `diff` and `visible` the same way other handlers do (look at how `V` / `select_next_file` are invoked).

- [ ] **Step 6: Add help entry**

In `src/ui.rs::draw_help_modal`, under **Diff — Shared** or **Diff — Sidebar**:

```rust
("U", "Jump to next unviewed file"),
```

- [ ] **Step 7: Verify**

Run: `cargo test --lib && cargo clippy && cargo run`
Expected: with mixed viewed/unviewed files, `U` cycles through unviewed ones in order.

- [ ] **Step 8: Commit**

```bash
git add src/app.rs src/main.rs src/ui.rs
git commit -m "Add U keybinding to jump to next unviewed file"
```

---

### Task 5: Status letter badge

The single-letter status (`A`/`M`/`D`/`R`) in the file row uses only `fg_color` (`ui.rs:177-187`). Render it as a cell-background badge for stronger visual weight.

**Files:**
- Modify: `src/ui.rs::draw_sidebar` (file arm)

Visual only; no new unit tests.

- [ ] **Step 1: Apply background and bold modifier**

In `src/ui.rs::draw_sidebar` file arm, replace the existing badge construction:

```rust
let (status_char, status_color) = match &file.status {
    model::FileStatus::Added => ("A", colors.fg_added),
    model::FileStatus::Modified => ("M", colors.fg_accent),
    model::FileStatus::Deleted => ("D", colors.fg_removed),
    model::FileStatus::Renamed { .. } => ("R", colors.fg_info),
};
let status_badge_text = format!(" {} ", status_char);
let status_badge_style = Style::default()
    .fg(Color::Black)
    .bg(status_color)
    .add_modifier(Modifier::BOLD);
let status_badge_span = Span::styled(status_badge_text, status_badge_style);
```

Replace the previous `Span::styled(status_badge, …)` push in `prefix_spans` with `status_badge_span`. Account for the extra 2 chars of width (the surrounding spaces) when computing the right-align padding.

- [ ] **Step 2: Verify**

Run: `cargo test --lib && cargo clippy && cargo run`
Expected: status now reads as `▓A▓` / `▓M▓` etc. with a coloured background block and bold black letter.

- [ ] **Step 3: Commit**

```bash
git add src/ui.rs
git commit -m "Render file status as colored cell badge in sidebar"
```

---

### Task 6: Sort options

Cycle through `Path | Churn | Status | UnviewedFirst` with `o`. Default is `Path` (current behaviour). Display the active sort mode in the sidebar title.

**Files:**
- Modify: `src/app.rs` (enum + state + cycle method + tests)
- Modify: `src/tree.rs` (`flatten` takes a sort param)
- Modify: `src/ui.rs` (use sort param; show indicator in title)
- Modify: `src/main.rs` (key binding)

- [ ] **Step 1: Define the enum and cycle method (TDD)**

Append to `src/app.rs` tests module:

```rust
#[test]
fn sort_order_cycles_through_all_variants() {
    let mut app = App::new();
    assert_eq!(app.sort_order, crate::app::SortOrder::Path);
    app.cycle_sort_order();
    assert_eq!(app.sort_order, crate::app::SortOrder::Churn);
    app.cycle_sort_order();
    assert_eq!(app.sort_order, crate::app::SortOrder::Status);
    app.cycle_sort_order();
    assert_eq!(app.sort_order, crate::app::SortOrder::UnviewedFirst);
    app.cycle_sort_order();
    assert_eq!(app.sort_order, crate::app::SortOrder::Path); // wraps
}
```

Run: `cargo test --lib sort_order_cycles`
Expected: FAIL.

In `src/app.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Path,
    Churn,
    Status,
    UnviewedFirst,
}

impl Default for SortOrder {
    fn default() -> Self { SortOrder::Path }
}

impl SortOrder {
    pub fn label(self) -> &'static str {
        match self {
            SortOrder::Path => "path",
            SortOrder::Churn => "churn",
            SortOrder::Status => "status",
            SortOrder::UnviewedFirst => "unviewed",
        }
    }
}
```

Add to `App`:

```rust
pub sort_order: SortOrder,
```

And in `App::new`:

```rust
sort_order: SortOrder::default(),
```

Add the cycle method:

```rust
pub fn cycle_sort_order(&mut self) {
    self.sort_order = match self.sort_order {
        SortOrder::Path => SortOrder::Churn,
        SortOrder::Churn => SortOrder::Status,
        SortOrder::Status => SortOrder::UnviewedFirst,
        SortOrder::UnviewedFirst => SortOrder::Path,
    };
}
```

Run: `cargo test --lib sort_order_cycles`
Expected: pass.

- [ ] **Step 2: Plumb the sort parameter through `FileTree::flatten`**

Write failing tests in `src/tree.rs` tests module:

```rust
#[test]
fn sort_by_churn_orders_highest_first() {
    let diff = make_diff(vec![
        make_file("low.rs", vec![make_hunk("@@", vec![
            make_line(LineKind::Added, "x", None, Some(1)),
        ])]),
        make_file("high.rs", vec![make_hunk("@@", vec![
            make_line(LineKind::Added, "a", None, Some(1)),
            make_line(LineKind::Added, "b", None, Some(2)),
            make_line(LineKind::Removed, "c", Some(1), None),
        ])]),
    ]);
    let tree = FileTree::from_files(&diff.files);
    let flat = tree.flatten_sorted(&HashSet::new(), &diff, crate::app::SortOrder::Churn);
    assert_eq!(entry_name(&flat[0]), "high.rs");
    assert_eq!(entry_name(&flat[1]), "low.rs");
}

#[test]
fn sort_by_unviewed_puts_unviewed_first() {
    let mut diff = make_diff(vec![
        make_file("a.rs", vec![]),
        make_file("b.rs", vec![]),
    ]);
    diff.files[0].viewed = true;
    let tree = FileTree::from_files(&diff.files);
    let flat = tree.flatten_sorted(&HashSet::new(), &diff, crate::app::SortOrder::UnviewedFirst);
    assert_eq!(entry_name(&flat[0]), "b.rs"); // unviewed first
    assert_eq!(entry_name(&flat[1]), "a.rs");
}
```

Run: `cargo test --lib sort_by_`
Expected: FAIL — `flatten_sorted` not found.

Add the new method to `FileTree`:

```rust
pub fn flatten_sorted(
    &self,
    collapsed: &HashSet<String>,
    diff: &crate::model::Diff,
    order: crate::app::SortOrder,
) -> Vec<FlatEntry> {
    let mut flat = self.flatten(collapsed);
    match order {
        crate::app::SortOrder::Path => {} // already alphabetical
        crate::app::SortOrder::Churn => flat.sort_by_key(|e| match &e.kind {
            FlatEntryKind::File { file_index, .. } => {
                let f = &diff.files[*file_index];
                std::cmp::Reverse(f.added + f.removed)
            }
            FlatEntryKind::Folder { path, .. } => {
                let (a, r) = crate::ui::folder_stats_pub(diff, path);
                std::cmp::Reverse(a + r)
            }
        }),
        crate::app::SortOrder::Status => flat.sort_by_key(|e| match &e.kind {
            FlatEntryKind::File { file_index, .. } => status_rank(&diff.files[*file_index].status),
            FlatEntryKind::Folder { .. } => 100, // folders last
        }),
        crate::app::SortOrder::UnviewedFirst => flat.sort_by_key(|e| match &e.kind {
            FlatEntryKind::File { file_index, .. } => diff.files[*file_index].viewed as u8,
            FlatEntryKind::Folder { .. } => 0,
        }),
    }
    flat
}

fn status_rank(s: &crate::model::FileStatus) -> u8 {
    match s {
        crate::model::FileStatus::Added => 0,
        crate::model::FileStatus::Modified => 1,
        crate::model::FileStatus::Renamed { .. } => 2,
        crate::model::FileStatus::Deleted => 3,
    }
}
```

Notes:
- Non-`Path` sorts intentionally flatten the tree (sort discards the folder hierarchy) — this is the GitHub web behaviour for sort modes. Tree connectors (Plan 1 Task 7) should not be drawn when `order != Path`. We'll handle that in the renderer (Step 3).
- `folder_stats` is currently private (`ui.rs:127`). Make a `pub(crate) fn folder_stats_pub` wrapper in `ui.rs` that calls it, or change visibility of the existing one.

Run: `cargo test --lib sort_by_`
Expected: pass.

- [ ] **Step 3: Call `flatten_sorted` in `main.rs` and adjust renderer**

Find where `flatten` is called in `src/main.rs`:

```bash
rg 'flatten\(' src/main.rs
```

Replace with `flatten_sorted(&app.collapsed, diff, app.sort_order)`.

In `src/ui.rs::draw_sidebar`:
- Append the active sort mode to the title from Plan 1 Task 1:

```rust
let summary = crate::app::diff_summary(diff);
let title = format!(
    " Files  {}/{} viewed  +{} -{}  · sort: {} ",
    summary.viewed, summary.total, summary.added, summary.removed,
    app.sort_order.label(),
);
```

- When `app.sort_order != SortOrder::Path`, skip tree connectors (the hierarchy is meaningless) — replace the `tree_prefix(entry)` call with an empty string in that branch.

- [ ] **Step 4: Bind `o`**

In `src/main.rs` diff-tab sidebar key handling:

```rust
KeyCode::Char('o') => app.cycle_sort_order(),
```

- [ ] **Step 5: Add help entry**

In `src/ui.rs::draw_help_modal`, under **Diff — Sidebar**:

```rust
("o", "Cycle sort: path · churn · status · unviewed"),
```

- [ ] **Step 6: Verify**

Run: `cargo test --lib && cargo clippy`
Expected: pass.

Run: `cargo run`
Expected: pressing `o` cycles the title's `sort: …` indicator, the sidebar re-orders, and tree connectors disappear in non-path modes.

- [ ] **Step 7: Update `docs/ubiquitous-language.md`**

Add to the **File Tree** section:

```markdown
| **SortOrder** | How sidebar entries are ordered: `Path` (alphabetical, tree-shaped — default), `Churn` (most-changed first, flat), `Status` (added → modified → renamed → deleted, flat), `UnviewedFirst` (unviewed before viewed, flat). |
```

- [ ] **Step 8: Commit**

```bash
git add src/app.rs src/tree.rs src/ui.rs src/main.rs docs/ubiquitous-language.md
git commit -m "Sidebar sort orders: o cycles path / churn / status / unviewed"
```

---

## Wrap-up

After all six tasks:

- [ ] Run the full test suite: `cargo test --lib`
- [ ] Run clippy: `cargo clippy --all-targets`
- [ ] Run `cargo run` and exercise:
  - Visit a branch with a renamed file → arrow visible
  - Tab through files → coloured churn bar matches the `+X -Y` count
  - Press `<` and `>` repeatedly → sidebar resizes, clamps at limits
  - Mark some files viewed, press `U` → jumps to next unviewed, wraps
  - Status badges read as coloured blocks (not plain letters)
  - Press `o` four times → title indicator cycles and the list re-sorts

If you want, update `CLAUDE.md`'s "Module Responsibilities" to note `App::sidebar_width`, `App::sort_order`, the new `FileTree::flatten_sorted`, and the new keys (`<`, `>`, `U`, `o`).
