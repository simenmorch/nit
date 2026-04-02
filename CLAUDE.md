# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

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
cargo test                   # run tests (no test suite yet)
```

GitHub PR mode requires a token: set `GITHUB_TOKEN` or have `gh` CLI authenticated.

## Architecture

Terminal-based code review tool. Two-panel TUI showing a file list sidebar and unified diff view with syntax highlighting.

```
CLI (clap) → Provider (git2 local | GitHub API) → Diff model → TUI (ratatui)
```

**Data flow:** `main.rs` parses the CLI arg to determine the diff source, fetches the diff into a `model::Diff` (a tree: files → hunks → lines), then enters the ratatui event loop. `App` owns all mutable UI state; `ui::draw()` reads it without mutation.

**Key abstraction — `ReviewProvider` trait** (`provider.rs`): Defines `fetch_diff`, `fetch_comments`, `fetch_metadata`. Currently implemented by `GitHubProvider`. Local git diffs bypass the trait and use `git.rs` functions directly. Future providers (GitLab, etc.) implement this trait.

**Async quirk:** `GitHubProvider` embeds its own `tokio::Runtime` and uses `rt.block_on()` to call octocrab's async API from synchronous trait methods. Octocrab's client must also be built inside `rt.block_on()` because tower (used internally) requires a tokio runtime context.

## Module Responsibilities

- **`main.rs`** — CLI parsing, diff source dispatch, event loop with keybinding handling
- **`model.rs`** — Pure data structs (`Diff`, `DiffFile`, `Hunk`, `Line`), no behavior
- **`git.rs`** — All libgit2 interaction: uncommitted diffs, commit diffs, range diffs, remote URL parsing
- **`github.rs`** — `ReviewProvider` impl for GitHub, includes patch text parser (`parse_patch`)
- **`app.rs`** — UI state (`App` struct): selection, scroll, focus, search. All state mutation lives here.
- **`ui.rs`** — Stateless rendering. Reads `App` + `Diff`, draws sidebar + diff + status bar. Owns syntax highlighting via syntect.
- **`provider.rs`** — `ReviewProvider` trait + shared types (`Comment`, `PrMetadata`, `PrState`)
- **`config.rs`** — TOML config loading from `~/.config/nit/nit.toml`

## Design Conventions

- Error handling uses `anyhow::Result` with `.context()` throughout. Fatal errors (no repo, no token) print to stderr and exit before TUI launch.
- State and rendering are strictly separated: `App` handles all state transitions, `ui::draw()` is a pure read. This is intentional for testability.
- Diff context is 5 lines (not git's default 3) to match GitHub's style.
- `Option<usize>` for line numbers — no sentinel values. `None` means the line doesn't exist in that version.
