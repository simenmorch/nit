# Sidebar Overview Improvements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the file sidebar a true at-a-glance overview by adding: review-progress summary in the title, right-aligned stats column, full-width selection background, scrollbar, file count on collapsed folders, single-child folder-chain collapsing, and tree connector glyphs.

**Architecture:** Most rendering lives in `ui.rs::draw_sidebar`. Tree-shape changes (chain collapsing, sibling info for connectors) live in `tree.rs::FileTree::flatten`. We add pure helpers (summary string, folder counts, tree-guide prefix) that are unit-tested directly; selection background, scrollbar, and connector rendering are verified by running `cargo run` against this repo's working tree.

**Tech Stack:** Rust, ratatui (`Block`, `Paragraph`, `Scrollbar`, `Span`), syntect (unchanged), `BTreeMap`-backed `FileTree`.

**Order rationale:** Information density first (1–4), then structural changes (5–7). Tasks 6 (chain collapsing) and 7 (tree connectors) both touch `FlatEntry`, so 6 comes first to lock in the final shape before connectors are computed.

---

### Task 1: Summary helper and sidebar title

The status bar (`ui.rs:1214-1268`) computes `viewed_count`, `total_added`, `total_removed`. Lift that into a pure helper so the same data can be reused in the sidebar title.

**Files:**
- Create helper: `src/app.rs` (add `pub fn diff_summary` as free function or `Diff` impl-like helper module)
- Modify: `src/ui.rs:134-220` (`draw_sidebar`) — use the summary in the block title

- [ ] **Step 1: Write the failing test**

Append to `src/app.rs` tests module:

```rust
#[test]
fn diff_summary_counts_viewed_added_removed() {
    let mut diff = make_diff(vec![
        make_file("a.rs", vec![make_hunk("@@", vec![
            make_line(LineKind::Added, "x", None, Some(1)),
            make_line(LineKind::Added, "y", None, Some(2)),
            make_line(LineKind::Removed, "z", Some(1), None),
        ])]),
        make_file("b.rs", vec![make_hunk("@@", vec![
            make_line(LineKind::Added, "p", None, Some(1)),
        ])]),
    ]);
    diff.files[0].viewed = true;

    let s = crate::app::diff_summary(&diff);
    assert_eq!(s.viewed, 1);
    assert_eq!(s.total, 2);
    assert_eq!(s.added, 3);
    assert_eq!(s.removed, 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib diff_summary_counts_viewed_added_removed`
Expected: FAIL — `cannot find function diff_summary`.

- [ ] **Step 3: Implement the helper**

Append to `src/app.rs`:

```rust
pub struct DiffSummary {
    pub viewed: usize,
    pub total: usize,
    pub added: usize,
    pub removed: usize,
}

pub fn diff_summary(diff: &crate::model::Diff) -> DiffSummary {
    DiffSummary {
        viewed: diff.files.iter().filter(|f| f.viewed).count(),
        total: diff.files.len(),
        added: diff.files.iter().map(|f| f.added).sum(),
        removed: diff.files.iter().map(|f| f.removed).sum(),
    }
}
```

- [ ] **Step 4: Use it in the sidebar title**

In `src/ui.rs` modify `draw_sidebar` (`ui.rs:210-218`):

```rust
let summary = crate::app::diff_summary(diff);
let title = format!(
    " Files  {}/{} viewed  +{} -{} ",
    summary.viewed, summary.total, summary.added, summary.removed,
);

let file_list = Paragraph::new(lines)
    .scroll((app.sidebar_scroll as u16, 0))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(title),
    );
```

- [ ] **Step 5: Run tests and verify**

Run: `cargo test --lib && cargo clippy`
Expected: all pass, no warnings.

Run: `cargo run`
Expected: sidebar title reads e.g. `─ Files  0/3 viewed  +12 -4 ─`.

- [ ] **Step 6: Commit**

```bash
git add src/app.rs src/ui.rs
git commit -m "Sidebar title shows viewed/total and +/- summary"
```

---

### Task 2: Right-align the stats column

Currently the `+X -Y` text floats right after the filename so it jiggles with name length. Pad so stats sit flush right inside the sidebar border.

**Files:**
- Modify: `src/ui.rs:144-202` (`draw_sidebar` per-row construction)

This task has no easily-unit-testable surface (rendering depends on `Frame`), so we test the padding-math helper.

- [ ] **Step 1: Write the failing test**

Append to `src/ui.rs` tests module (create one if it doesn't exist) or to `src/app.rs` if you prefer to keep `ui.rs` test-free:

```rust
#[test]
fn stats_padding_fills_to_right_edge() {
    // inner_width 40, prefix takes 20, stats text "+12 -3" is 6 chars
    let padding = crate::ui::stats_padding(40, 20, 6);
    assert_eq!(padding, 14);
}

#[test]
fn stats_padding_clamps_when_overflow() {
    let padding = crate::ui::stats_padding(20, 25, 6);
    assert_eq!(padding, 0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib stats_padding`
Expected: FAIL — function not found.

- [ ] **Step 3: Implement the helper and use it**

In `src/ui.rs` add (above `draw_sidebar`):

```rust
pub(crate) fn stats_padding(inner_width: usize, prefix_width: usize, stats_width: usize) -> usize {
    inner_width
        .saturating_sub(prefix_width)
        .saturating_sub(stats_width)
}
```

Then rewrite each row builder in `draw_sidebar` to:
1. Build the left-side spans (marker, indent, arrow/badge, name) and the stats string separately.
2. Compute display width of the left side (sum of `span.content.chars().count()` over each `Span`).
3. Insert a single `Span::raw(" ".repeat(padding))` between left and stats.

Pass `inner_width = area.width.saturating_sub(2) as usize` into the row builder.

Concretely, in `draw_sidebar` before the `.map` closure:

```rust
let inner_width = area.width.saturating_sub(2) as usize;
```

Inside each match arm, after building `prefix_spans` (everything up to and including the name) and the `stats` string:

```rust
let prefix_width: usize = prefix_spans.iter().map(|s| s.content.chars().count()).sum();
let pad = stats_padding(inner_width, prefix_width, stats.chars().count());
let mut spans = prefix_spans;
spans.push(Span::raw(" ".repeat(pad)));
spans.push(Span::styled(stats, Style::default().fg(colors.fg_muted)));
Line::from(spans)
```

- [ ] **Step 4: Run tests and verify**

Run: `cargo test --lib && cargo clippy`
Expected: pass.

Run: `cargo run`
Expected: all `+X -Y` figures form a clean vertical column at the right edge of the sidebar.

- [ ] **Step 5: Commit**

```bash
git add src/ui.rs
git commit -m "Right-align sidebar stats column"
```

---

### Task 3: Full-width selection background

Selection currently signals only with a `▸ ` marker and a foreground colour change. Apply `bg(highlight_bg)` to every span of the selected row, including the padding span from Task 2, so the highlight spans the full sidebar width. After this we can drop the leading `▸ ` marker (reclaim 2 columns).

**Files:**
- Modify: `src/ui.rs:144-202` (`draw_sidebar`)

This change is visual; tests are not added. Test by running.

- [ ] **Step 1: Add a `bg_selected` colour to `ColorsConfig`**

In `src/config.rs:38-73` add the field with hex deserializer, and in `Default for ColorsConfig` (`config.rs:75-97`) give it a value:

```rust
#[serde(deserialize_with = "deserialize_hex_color")]
pub bg_selected: Color,
```

```rust
bg_selected: Color::Rgb(40, 50, 70),
```

Add the field to the `full_config_parse` test TOML (`config.rs:278-300`) so it stays representative:

```text
bg_selected = "#283246"
```

Update the `bg_split_empty` line above it likewise (it already exists). Run `cargo test --lib full_config_parse` and confirm it passes.

- [ ] **Step 2: Apply the background to selected rows**

In `src/ui.rs::draw_sidebar`, after building the row spans for a selected entry, map them to apply `bg`:

```rust
let highlight_bg = colors.bg_selected;
if is_selected {
    spans = spans
        .into_iter()
        .map(|s| Span::styled(s.content.to_string(), s.style.bg(highlight_bg)))
        .collect();
}
```

Then **remove the `marker` (`"▸ "` / `"  "`) entirely** from the row construction — it's redundant with the background. Update `prefix_width` accordingly (subtract 2 chars or just rebuild the vec from scratch).

- [ ] **Step 3: Verify**

Run: `cargo test --lib && cargo clippy`
Expected: pass.

Run: `cargo run`
Expected: the selected file or folder row has a solid background spanning the entire sidebar width, no `▸ ` marker.

- [ ] **Step 4: Commit**

```bash
git add src/config.rs src/ui.rs
git commit -m "Full-width background on selected sidebar row; drop ▸ marker"
```

---

### Task 4: Sidebar scrollbar

When the entry list overflows the sidebar height the user has no idea where they are in it. Reuse `render_scrollbar` (`ui.rs:1511-1533`).

**Files:**
- Modify: `src/ui.rs::draw_sidebar` (`ui.rs:134-220`)

- [ ] **Step 1: Add scrollbar render call**

At the end of `draw_sidebar`, after `frame.render_widget(file_list, area);`, add:

```rust
let inner_height = area.height.saturating_sub(2) as usize;
render_scrollbar(frame, area, app.sidebar_scroll, visible.len(), inner_height, colors);
```

`render_scrollbar` already returns early when content fits, so it's a no-op for short lists.

- [ ] **Step 2: Verify**

Run: `cargo test --lib && cargo clippy`
Expected: pass.

Run: `cargo run` against a branch with > sidebar-height files (or shrink terminal height). 
Expected: a scrollbar appears on the right edge of the sidebar, thumb tracks `j`/`k` movement.

- [ ] **Step 3: Commit**

```bash
git add src/ui.rs
git commit -m "Render scrollbar in sidebar when entries overflow"
```

---

### Task 5: File count on folders

Show `(N)` or `(N, M✓)` next to folder names so collapsed folders aren't opaque.

**Files:**
- Modify: `src/ui.rs` (add helper, use in `draw_sidebar` folder arm)

- [ ] **Step 1: Write the failing test**

Append to `src/ui.rs` tests module:

```rust
#[test]
fn folder_file_counts_total_and_viewed() {
    let mut diff = make_diff(vec![
        make_file("src/a.rs", vec![]),
        make_file("src/b.rs", vec![]),
        make_file("src/c.rs", vec![]),
        make_file("other/d.rs", vec![]),
    ]);
    diff.files[0].viewed = true;
    diff.files[2].viewed = true;

    let (total, viewed) = crate::ui::folder_file_counts(&diff, "src");
    assert_eq!(total, 3);
    assert_eq!(viewed, 2);
}

#[test]
fn folder_file_counts_excludes_other_folders() {
    let diff = make_diff(vec![
        make_file("src/a.rs", vec![]),
        make_file("src-other/b.rs", vec![]), // shares prefix string, not folder
    ]);
    let (total, _) = crate::ui::folder_file_counts(&diff, "src");
    assert_eq!(total, 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib folder_file_counts`
Expected: FAIL — function not found.

- [ ] **Step 3: Implement the helper**

In `src/ui.rs` near `folder_stats` (`ui.rs:127-132`):

```rust
pub(crate) fn folder_file_counts(diff: &model::Diff, folder_path: &str) -> (usize, usize) {
    let prefix = format!("{}/", folder_path);
    let files: Vec<_> = diff.files.iter().filter(|f| f.path.starts_with(&prefix)).collect();
    let viewed = files.iter().filter(|f| f.viewed).count();
    (files.len(), viewed)
}
```

- [ ] **Step 4: Render the count**

In `draw_sidebar`'s folder arm, after computing `stats`:

```rust
let (count, viewed_count) = folder_file_counts(diff, path);
let count_str = if viewed_count > 0 {
    format!(" ({}, {}✓)", count, viewed_count)
} else {
    format!(" ({})", count)
};
```

Insert a span with `count_str` styled `colors.fg_muted` right after the folder name span, before the padding.

- [ ] **Step 5: Verify**

Run: `cargo test --lib && cargo clippy`
Expected: pass.

Run: `cargo run`
Expected: each folder row shows e.g. `src/ (8, 3✓)` to the right of the name.

- [ ] **Step 6: Commit**

```bash
git add src/ui.rs
git commit -m "Show file count (and viewed count) on folder rows"
```

---

### Task 6: Collapse single-child folder chains

A file at `crates/server/src/handlers/auth.rs` currently produces five indented rows before the filename. Merge any folder F that contains exactly one entry which is itself a folder C, into a single row labelled `F/C`. Recurse. Files are never merged — leaves remain on their own row.

**Files:**
- Modify: `src/tree.rs:31-119` (`FileTree::from_files` / `flatten`)
- Tests: `src/tree.rs` tests module

Approach: do the merging during `flatten` (cheap, no struct churn). For each folder entry being emitted, walk down while it has a single folder child and accumulate path segments into the displayed name.

- [ ] **Step 1: Write failing tests**

Append to `src/tree.rs` tests module:

```rust
#[test]
fn single_folder_child_chain_is_merged() {
    let diff = make_diff(vec![make_file("a/b/c/leaf.rs", vec![])]);
    let flat = flat_entries_for(&diff);
    // 2 entries: "a/b/c" folder at depth 0, "leaf.rs" file at depth 1
    assert_eq!(flat.len(), 2);
    assert!(is_folder(&flat[0]));
    assert_eq!(entry_name(&flat[0]), "a/b/c");
    assert_eq!(flat[0].depth, 0);
    assert_eq!(entry_name(&flat[1]), "leaf.rs");
    assert_eq!(flat[1].depth, 1);
}

#[test]
fn chain_breaks_when_folder_has_multiple_children() {
    // src/ has two children (foo/ and bar.rs) so src/ does NOT merge with foo.
    let diff = make_diff(vec![
        make_file("src/foo/leaf.rs", vec![]),
        make_file("src/bar.rs", vec![]),
    ]);
    let flat = flat_entries_for(&diff);
    // src/ folder, then foo/ folder (single file child → not merged), then leaf.rs, then bar.rs
    assert_eq!(flat.len(), 4);
    assert_eq!(entry_name(&flat[0]), "src");
    assert_eq!(entry_name(&flat[1]), "foo");
    assert_eq!(entry_name(&flat[2]), "leaf.rs");
    assert_eq!(entry_name(&flat[3]), "bar.rs");
}

#[test]
fn collapsed_chain_path_is_full_for_collapse_toggle() {
    // Folder path stored in FlatEntryKind::Folder must be the merged path
    // so that toggle_collapse / collapsed HashSet keying still works.
    let diff = make_diff(vec![
        make_file("a/b/x.rs", vec![]),
        make_file("a/b/y.rs", vec![]),
    ]);
    let flat = flat_entries_for(&diff);
    // a/b folder (merged), then x.rs, then y.rs
    assert_eq!(flat.len(), 3);
    if let FlatEntryKind::Folder { path, name, .. } = &flat[0].kind {
        assert_eq!(path, "a/b");
        assert_eq!(name, "a/b");
    } else {
        panic!("expected folder");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib --no-fail-fast tree::tests`
Expected: the three new tests fail with assertion mismatches on length / names.

- [ ] **Step 3: Implement chain merging in `flatten_inner`**

Replace the folder loop in `src/tree.rs:83-103` with chain-collapsing logic:

```rust
for (name, entry) in folders {
    if let TreeEntry::Folder(subtree) = entry {
        // Walk down single-folder chains
        let mut merged_segments: Vec<String> = vec![name.clone()];
        let mut current = subtree;
        loop {
            if current.entries.len() != 1 {
                break;
            }
            let (only_name, only_entry) = current.entries.iter().next().unwrap();
            match only_entry {
                TreeEntry::Folder(sub) => {
                    merged_segments.push(only_name.clone());
                    current = sub;
                }
                TreeEntry::File(_) => break,
            }
        }

        let display_name = merged_segments.join("/");
        let full_path = if prefix.is_empty() {
            display_name.clone()
        } else {
            format!("{}/{}", prefix, display_name)
        };
        let expanded = !collapsed.contains(&full_path);
        result.push(FlatEntry {
            depth,
            kind: FlatEntryKind::Folder {
                path: full_path.clone(),
                name: display_name,
                expanded,
            },
        });
        if expanded {
            result.extend(current.flatten_inner(collapsed, &full_path, depth + 1));
        }
    }
}
```

- [ ] **Step 4: Re-run tests**

Run: `cargo test --lib`
Expected: all tests pass, including the existing `nested_file_creates_folder`, `deeply_nested`, `collapse_inner_folder_keeps_outer`.

If `deeply_nested` (expects 4 rows for `a/b/c/d.rs`) now fails, update it to reflect the new behaviour:

```rust
#[test]
fn deeply_nested() {
    let diff = make_diff(vec![make_file("a/b/c/d.rs", vec![])]);
    let flat = flat_entries_for(&diff);
    // Chain collapses: "a/b/c" folder + "d.rs" file
    assert_eq!(flat.len(), 2);
    assert_eq!(entry_name(&flat[0]), "a/b/c");
    assert_eq!(flat[0].depth, 0);
    assert_eq!(entry_name(&flat[1]), "d.rs");
    assert_eq!(flat[1].depth, 1);
}
```

Similarly, audit `collapse_inner_folder_keeps_outer` and `shared_prefix_single_folder` — they should still pass (multiple children prevent merging).

- [ ] **Step 5: Verify visually**

Run: `cargo run`
Expected: deeply-nested files show as e.g. `crates/server/src/handlers/` (one folder row) + `auth.rs` (file row) instead of five indented rows.

- [ ] **Step 6: Commit**

```bash
git add src/tree.rs
git commit -m "Collapse single-child folder chains in sidebar"
```

---

### Task 7: Tree connector glyphs

Replace the bare `"  ".repeat(depth)` indent with proper tree connectors (`│  `, `├─ `, `└─ `) so hierarchy is visually obvious.

**Files:**
- Modify: `src/tree.rs` (`FlatEntry` gets sibling info; `flatten_inner` computes it)
- Modify: `src/ui.rs::draw_sidebar` (render the prefix)
- Tests: `src/tree.rs`

- [ ] **Step 1: Extend `FlatEntry`**

In `src/tree.rs:14-29`:

```rust
pub struct FlatEntry {
    pub depth: usize,
    pub kind: FlatEntryKind,
    /// For each ancestor depth (0..depth), true if that ancestor has more
    /// siblings after this branch — meaning a vertical `│` should be drawn
    /// in that column. Length == depth.
    pub ancestor_has_next: Vec<bool>,
    /// True if this entry is the last child at its own depth.
    pub is_last_sibling: bool,
}
```

- [ ] **Step 2: Compute the fields in `flatten_inner`**

Adjust the function signature to thread the ancestor stack:

```rust
fn flatten_inner(
    &self,
    collapsed: &HashSet<String>,
    prefix: &str,
    depth: usize,
    ancestor_has_next: &[bool],
) -> Vec<FlatEntry> { ... }
```

Inside the function:
- Compute `total = folders.len() + files.len()` (after chain-collapse logic, both lists from Task 6 are still local).
- For each folder/file at sibling-position `i`, `is_last_sibling = i + 1 == total`.
- Push an entry with `ancestor_has_next: ancestor_has_next.to_vec()` and the computed `is_last_sibling`.
- For folder recursion pass a new ancestor stack `[ancestor_has_next, &[!is_last_sibling]].concat()`.

Update the public `flatten` to pass `&[]`:

```rust
pub fn flatten(&self, collapsed: &HashSet<String>) -> Vec<FlatEntry> {
    self.flatten_inner(collapsed, "", 0, &[])
}
```

Make sure folder iteration counts both folders and files together for `is_last_sibling` since the files come after the folders.

- [ ] **Step 3: Write tests for sibling computation**

Append to `src/tree.rs` tests module:

```rust
#[test]
fn last_sibling_flag_correct_for_root() {
    let diff = make_diff(vec![
        make_file("a.rs", vec![]),
        make_file("b.rs", vec![]),
    ]);
    let flat = flat_entries_for(&diff);
    assert!(!flat[0].is_last_sibling); // a.rs
    assert!(flat[1].is_last_sibling);  // b.rs
}

#[test]
fn ancestor_has_next_drawn_for_non_last_parents() {
    // Two folders at root, each with one file:
    //   src/foo.rs       (src is not last → ancestor_has_next[] = []; for foo.rs depth=1 → ancestor_has_next = [true])
    //   tests/bar.rs     (tests is last → for bar.rs ancestor_has_next = [false])
    let diff = make_diff(vec![
        make_file("src/foo.rs", vec![]),
        make_file("tests/bar.rs", vec![]),
    ]);
    let flat = flat_entries_for(&diff);
    // [src, foo.rs, tests, bar.rs]
    assert_eq!(flat[1].ancestor_has_next, vec![true]);  // src has tests after it
    assert_eq!(flat[3].ancestor_has_next, vec![false]); // tests is last
}
```

Run: `cargo test --lib tree::tests`
Expected: fail until step 2 is correct, then pass.

- [ ] **Step 4: Render connectors in `draw_sidebar`**

Add a helper at the top of `src/ui.rs`:

```rust
fn tree_prefix(entry: &FlatEntry) -> String {
    let mut out = String::new();
    for &has_next in &entry.ancestor_has_next {
        out.push_str(if has_next { "│  " } else { "   " });
    }
    if entry.depth > 0 {
        out.push_str(if entry.is_last_sibling { "└─ " } else { "├─ " });
    }
    out
}
```

In `draw_sidebar`, replace the existing `let indent = "  ".repeat(entry.depth);` (and the analogous `file_indent` line) with:

```rust
let indent = tree_prefix(entry);
```

Remove the now-defunct `file_indent` adjustment that subtracted 1 from depth.

- [ ] **Step 5: Verify**

Run: `cargo test --lib && cargo clippy`
Expected: pass.

Run: `cargo run`
Expected: sidebar shows e.g.
```
 Files  0/4 viewed  +20 -8
├─ src/ (3, 0✓)              +15 -6
│  ├─ a.rs                    +5 -2
│  ├─ b.rs                    +5 -2
│  └─ c.rs                    +5 -2
└─ README.md                  +5 -2
```

- [ ] **Step 6: Update `docs/ubiquitous-language.md`**

In the **File Tree** section add a row:

```markdown
| **Tree Connector** | The `├─`, `└─`, `│  ` glyphs drawn before each sidebar entry to make the parent-child hierarchy visible. Computed from each `FlatEntry`'s `ancestor_has_next` and `is_last_sibling`. |
```

- [ ] **Step 7: Commit**

```bash
git add src/tree.rs src/ui.rs docs/ubiquitous-language.md
git commit -m "Draw tree connectors in sidebar"
```

---

## Wrap-up

After all seven tasks:

- [ ] Run the full test suite: `cargo test --lib`
- [ ] Run clippy: `cargo clippy --all-targets`
- [ ] Run the app against this repo (`cargo run`) and exercise: switch branches with `b`, navigate with `j`/`k`, toggle a folder with `Enter`, resize the terminal.
- [ ] Verify the sidebar title summary, right-aligned stats, full-row selection background, scrollbar (on overflow), folder counts, collapsed chains, and connector glyphs all look correct together.

If you want, update `CLAUDE.md`'s "Module Responsibilities" entry for `ui.rs` and `tree.rs` to mention the new helpers (`diff_summary`, `stats_padding`, `folder_file_counts`, `tree_prefix`, chain-collapsing in `flatten`).
