# Nit — Planning Document

A terminal-based code review tool built in Rust. Name comes from "nitpick" — the small, careful comments that make code better.

**Goal:** A GitHub-like "Files changed" experience in the terminal — structured, navigable, keyboard-driven.

**Inspiration:** gitui (bordered panels, contextual keybinding bar). Not a copy — just taking cues for clean, focused TUI design.

## Modes (full vision)

- `nit` — local uncommitted diff
- `nit abc123` — single commit
- `nit abc123..def456` — commit range
- `nit #42` — GitHub PR (current repo)
- `nit gh:owner/repo#42` — GitHub PR (any repo)

## Architecture (high level)

```
CLI (clap) → Diff Source trait (GitLocal | GitHubAPI) → Diff Model (files → hunks → lines) → TUI (Ratatui)
```

## Key Crates

- `ratatui` + `crossterm` — TUI framework
- `git2` — libgit2 bindings
- `octocrab` — GitHub API (async, later phases)
- `syntect` — syntax highlighting (later phases)
- `clap` — CLI args
- `tokio` — async runtime
- `directories` + `toml` — config (later phases)

---

## Working Agreement

This is a Rust learning project. The goal is not just a working tool but a codebase the author fully understands and can extend independently.

1. **Concept-first, code-second** — Explain the Rust concepts and design choices before writing code. Understand the "why" before the "what."
2. **Small, digestible steps** — One struct, one function, one trait at a time. Each unit of work should be small enough to fully read and understand.
3. **You write, I guide** — For key Rust learning moments (first trait, first lifetime, first enum with pattern matching, etc.), the author writes the code with guidance rather than having it generated. Much stickier than reading someone else's code.
4. **Review before moving on** — After each chunk, make sure it's understood before building on top of it. Questions are the point, not a distraction.
5. **Comments only where non-obvious** — When a Rust pattern is genuinely non-obvious (why we borrow here, why this is an enum), leave a brief comment. Don't over-annotate.

---

## Phase 1 — Local MVP

### Scope

**In:**
- Diff source: uncommitted changes only (`nit` with no args)
- Two-screen UI model: sidebar ↔ file diff view
- Sidebar: file list with diff stats (+/-), viewed checkmarks
- File diff view: unified diff, one file at a time, full screen
- Basic diff coloring (+green / -red), no syntax highlighting
- Vim-style navigation (j/k, gg/G)
- Bordered panel layout (gitui-inspired)
- Contextual keybinding bar at bottom
- Keyboard only — no mouse support

**Prepare for (but don't build):**
- Theming: define all colors in a single `Theme` struct or constants block. No hardcoded colors scattered through `ui.rs`. Makes it trivial to swap in configurable themes later.

**Out (deferred):**
- Syntax highlighting (Phase 1.5)
- Commit / commit range viewing (Phase 2+)
- GitHub integration (Phase 2-3)
- Side-by-side diff view (Phase 4)
- Collapse/expand (not needed with single-file view model)
- Mouse support
- Config file / themes
- Search within diffs

### Implementation Steps

Each step is one sitting — small enough to fully read, understand, and verify before moving on.

**Step 1 — Scaffold**
Set up `cargo new`, create module files (`model.rs`, `git.rs`, `app.rs`, `ui.rs`), add dependencies to `Cargo.toml`. Get it compiling with empty modules.
*Rust concepts:* project structure, `mod` / `use`, crate dependencies.

**Step 2 — Data model**
Write the structs and enums in `model.rs`: `LineKind`, `Line`, `Hunk`, `FileStatus`, `DiffFile`, `Diff`.
*Rust concepts:* enums (with and without associated data), `Option<T>`, struct definitions, `String` vs `&str`.

**Step 3 — Git layer**
Implement `git.rs`: open repo, read uncommitted diff via git2, produce a `Diff`. Write integration tests with temp repos.
*Rust concepts:* `Result<T>`, `?` operator, `anyhow::Context`, borrowing from library objects, iterators.

**Step 4 — CLI + main wiring**
Add clap arg parsing (just `nit` with no args for now). Wire up `main.rs`: open repo → build diff → print file count to verify it works end-to-end.
*Rust concepts:* clap basics, error handling in `main`, early returns.

**Step 5 — Minimal TUI**
Get a ratatui app loop running: init terminal, render a static "hello" screen, quit on `q`. No real data yet.
*Rust concepts:* terminal setup/teardown, event loop pattern, `crossterm` events.

**Step 6 — Sidebar rendering**
Draw the file list from real diff data: filenames, +/- stats, bordered panel. Static — no interaction yet.
*Rust concepts:* ratatui layouts, widgets, passing `&self` references, rendering from data.

**Step 7 — App state + navigation**
Add `App` struct in `app.rs` with cursor position and view state. Handle j/k to move selection in the sidebar. Write unit tests for state transitions.
*Rust concepts:* mutable state, `match` on key events, enum for view state, `&mut self`.

**Step 8 — File diff view**
Render hunks for the selected file on Enter. Scroll with j/k, return to sidebar with q/Esc. Toggle "viewed" flag.
*Rust concepts:* view transitions via enum, scroll offset, index into nested data (`files[i].hunks`).

**Step 9 — Polish**
Diff coloring (+green/-red) via `Theme` constants. Viewed checkmarks in sidebar. Status bar (branch, viewed count, total stats). Keybinding hint bar. gg/G jump navigation.
*Rust concepts:* ratatui `Style`, organizing constants, layout composition.

### UI Model

**Sidebar (home view):**
- List of changed files with diff stats and viewed checkmarks
- Navigate with j/k, select with Enter
- Keybinding bar at bottom showing available actions

**File diff view:**
- Full-screen unified diff for the selected file
- Scroll with j/k, jump with gg/G
- Press q or Esc to return to sidebar
- Toggle "viewed" from within the diff view
- Keybinding bar updates to show diff-view actions

---

## Data Model

```rust
enum LineKind {
    Added,
    Removed,
    Context,
}

struct Line {
    kind: LineKind,
    content: String,
    old_num: Option<usize>,  // None for added lines
    new_num: Option<usize>,  // None for removed lines
}

struct Hunk {
    header: String,          // the @@ line
    lines: Vec<Line>,
}

enum FileStatus {
    Modified,
    Added,
    Deleted,
    Renamed { from: String },
}

struct DiffFile {
    path: String,
    status: FileStatus,
    hunks: Vec<Hunk>,
    added: usize,            // +line count
    removed: usize,          // -line count
    viewed: bool,
}

struct Diff {
    files: Vec<DiffFile>,
}
```

**Design notes:**
- `LineKind` and `FileStatus` are enums — Rust's way of representing a fixed set of variants
- `Option<usize>` for line numbers: `None` means "doesn't exist in that version" (no sentinel values)
- `Renamed { from }` is an enum variant with associated data — carries the old filename
- `viewed` lives on `DiffFile` since it's per-file UI state
- Stats (`added`/`removed`) stored rather than computed — avoids recounting on every render

---

## Error Handling

**Crate:** `anyhow` — wraps errors with context messages, idiomatic for applications. Pairs with `Result<T>` and `?` operator.

**Fatal errors** (not a git repo, terminal init failure):
- Print to stderr and exit with non-zero code. Don't launch the TUI.

**Empty state** (no uncommitted changes):
- Launch the TUI, show "No uncommitted changes" message in the sidebar.

**Binary files:**
- Show in the sidebar file list, but display "Binary file changed" in the diff view instead of hunks.

**Internal git2 errors:**
- Wrap with `.context("description")` and propagate up with `?`. Fatal by default in Phase 1 — no partial recovery.

---

## Project Structure

Single crate, modules by responsibility. Maps directly to the architecture layers.

```
src/
  main.rs          # entry point, CLI args, error handling
  model.rs         # Diff, DiffFile, Hunk, Line, enums
  git.rs           # git2 interaction, produces Diff
  app.rs           # app state, key handling, view transitions
  ui.rs            # ratatui rendering (reads app state, draws)

Cargo.toml
docs/
  plan.md
```

**Module responsibilities:**
- `main.rs` — parse CLI args, open repo, build diff, launch TUI, handle fatal errors
- `model.rs` — data structures only, no behavior beyond constructors. Easy to test.
- `git.rs` — calls git2, produces a `Diff`. Isolates all git interaction.
- `app.rs` — owns app state (selected file, current view, cursor position). Handles key input, updates state. No rendering.
- `ui.rs` — reads `App` state, draws with ratatui. No state mutation.

Can split into sub-modules later (e.g., `ui/sidebar.rs`, `ui/diff_view.rs`) if files grow large.

---

## Testing Strategy

**Principle:** Separate state from rendering — makes logic testable without a terminal.

**What we test in Phase 1:**
- **Diff parsing** — Unit tests: input data → our structs. Pure functions, easy to test.
- **App state transitions** — Unit tests: simulate key presses, assert on state (cursor position, current view, viewed flags). No rendering involved.
- **Git integration** — Create temporary git repos in tests (temp dir, init, add files, modify), run our diff extraction against them.

**What we skip:**
- Rendering / layout tests (ratatui buffer snapshots). Too brittle for Phase 1, low value relative to effort.

**Architectural implication:** The app must separate state management from rendering. State struct holds data + handles input. Render functions read state and draw. This is the standard ratatui pattern.

---

## Additional Decisions

**Hunk context lines:** 5 lines of unchanged context around each change (slightly more than git's default of 3, closer to GitHub's view).

**Large diffs:** Truncate after a threshold with a message (e.g., "Diff truncated — file too large"). Exact threshold TBD during implementation — likely around 5,000 lines.

**Status bar layout:**
```
┌─ top: main content (sidebar or diff view) ─┐
│                                             │
└─────────────────────────────────────────────┘
 main | 3/5 files viewed | +42 -17     ← info bar
 j/k: navigate  Enter: open  q: quit   ← keybinding bar
```
Shows: branch name, viewed progress, total diff stats. Keybinding hints on a separate line below.

---

## Open Questions

All initial questions resolved. New questions may arise during implementation.

---

## Phased Roadmap (full vision)

1. **Phase 1 — Local MVP:** (see above)
2. **Phase 1.5 — Polish:** Syntax highlighting via syntect, commit/range viewing
3. **Phase 2 — GitHub read-only:** Fetch PR diffs/comments via octocrab, show inline
4. **Phase 3 — GitHub interactive:** Post comments, submit reviews, mark files viewed
5. **Phase 4 — Polish:** Side-by-side view, search, large file handling, config/keybindings
