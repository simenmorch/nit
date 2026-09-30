# AGENTS.md

This file provides guidance for AI coding agents when working with code in this repository.

## Build and Run

```bash
cargo build                  # dev build
cargo build --release        # release build
cargo run                    # run against uncommitted changes in current repo
cargo run -- abc123          # single commit
cargo run -- 'abc123..def456' # commit range
cargo run -- '#42'           # GitHub PR (current repo)
cargo run -- 'owner/repo#42' # GitHub PR (any repo)
cargo clippy                 # lint
cargo fmt                    # format (keep the tree clean)
cargo test                   # run all tests (202 unit + 5 integration)
cargo test --lib             # unit tests only
```

GitHub PR mode requires a token: set `GITHUB_TOKEN` or have `gh` CLI authenticated.

## Architecture

Terminal-based code review tool. Tabbed TUI with three tabs: Diff (file tree sidebar + unified/split diff view with syntax highlighting), Commits (log browser), and PRs (GitHub PR list). Supports in-app git operations: staging, committing, branch switching, pull, push.

```
CLI (clap) → Provider (git2 local | GitHub API) → Diff model → TUI (ratatui)
```

See [docs/ubiquitous-language.md](docs/ubiquitous-language.md) for definitions of key domain terms used throughout the codebase.

**Data flow:** `main.rs` parses the CLI arg to determine the diff source, fetches the diff into a `model::Diff` (a tree: files → hunks → lines), then enters the ratatui event loop. `App` owns all mutable UI state; `ui::draw()` reads it without mutation. In default (uncommitted) mode, the diff auto-refreshes every 2 seconds.

**Key abstraction — `RemoteProvider` trait** (`provider.rs`): Defines `fetch_diff`, `fetch_comments`, `fetch_metadata`, `fetch_pr_list`, `fetch_authenticated_user`. Currently implemented by `GitHubProvider`. Local git diffs bypass the trait and use `git.rs` functions directly. Future providers (GitLab, etc.) implement this trait.

**Async quirk:** `GitHubProvider` embeds its own `tokio::Runtime` and uses `rt.block_on()` to call octocrab's async API from synchronous trait methods. Octocrab's client must also be built inside `rt.block_on()` because tower (used internally) requires a tokio runtime context.

## Module Responsibilities

- **`main.rs`** — CLI parsing, diff source dispatch, event loop, keybinding handling (delegates to `handle_key` → tab-specific handlers), and git actions (commit, checkout, pull, push). Also manages `ReviewMode` transitions (working tree ↔ commit ↔ PR).
- **`model.rs`** — Pure data structs (`Diff`, `DiffFile`, `Hunk`, `Line`, `CommitInfo`, `PrInfo`), no behavior.
- **`git.rs`** — All libgit2 interaction: uncommitted/staged/commit/range diffs, staging, committing, branch listing/checkout, pull/push (shells out for push/pull), remote URL parsing, commit log.
- **`github.rs`** — `RemoteProvider` impl for GitHub via octocrab. Includes patch text parser (`parse_patch`).
- **`app.rs`** — UI state (`App` struct): selection, scroll, focus, search, view mode, tabs, modals (commit, branch, confirm, help, PR filter). All state mutation lives here.
- **`ui.rs`** — Stateless rendering. Reads `App` + `Diff`, draws tab bar, sidebar, diff (unified + split), commit list, PR list, modals, status bar. Owns syntax highlighting via syntect.
- **`split.rs`** — Side-by-side diff logic: converts hunks into `SplitRow`s (paired/left-only/right-only) with character-level inline diff spans via the `similar` crate.
- **`cache.rs`** — Precomputes and caches per-file split-view layout metadata (`DiffCache`) to avoid redundant recomputations in the event loop.
- **`tree.rs`** — `FileTree`: builds a hierarchical tree from flat file paths, flattens it for rendering with collapsible folders (folders sorted before files) and collapsed single-child folder chains. Folders and files are kept in separate maps so a segment can be both (e.g. deleting file `foo` while adding `foo/bar.rs`).
- **`provider.rs`** — `RemoteProvider` trait + shared types (`Comment`, `PrMetadata`, `PrState`).
- **`config.rs`** — TOML config loading from `~/.config/nit/nit.toml`. Supports syntax theme selection (bundled, `.tmTheme`, `.sublime-color-scheme` files in `~/.config/nit/themes/`) and full color customization via hex values.
- **`lib.rs`** — Crate root, re-exports all modules.
- **`test_helpers.rs`** — Shared test factories (`make_line`, `make_hunk`, `make_file`, `make_diff`, `make_simple_diff`, `flat_entries_for`). Only compiled in `#[cfg(test)]`.

## Design Conventions

- Error handling uses `anyhow::Result` with `.context()` throughout. Fatal errors (no repo, no token) print to stderr and exit before TUI launch.
- State and rendering are strictly separated: `App` handles all state transitions, `ui::draw()` is a pure read. This is intentional for testability.
- Diff context is 5 lines (not git's default 3) to match GitHub's style — see `git::CONTEXT_LINES`, applied via `git::diff_opts()` so it cannot drift between diff paths.
- All local diffs run through `git::finish_diff`, which calls `find_similar` — libgit2 never reports renames without it.
- `Option<usize>` for line numbers — no sentinel values. `None` means the line doesn't exist in that version.
- `viewed` on `DiffFile` maps to git staging in default (uncommitted) mode — toggling viewed stages/unstages the file. `toggle_viewed_entry` returns a `ViewedChange` describing both directions; `main::sync_index_with_viewed` applies it and rolls the flag back if the index operation fails, so the UI never claims a file is staged when it is not.
- `file_index` in `FlatEntry` is *positional*. After the diff is replaced, use `App::resync_selection` to re-resolve the selection by path — clamping alone would silently point at a different file.
- Split-view layout data is precomputed into `DiffCache` on diff load/refresh to avoid redundant recomputations in the event loop.
- Column layout in `ui.rs` uses `display_width`/`truncate_to_width`/`pad_to_width` (unicode-width), not `str::len` or `chars().count()`. Both disagree with what ratatui measures.
- The event loop drains all queued key events before redrawing, for responsiveness.
