# Ubiquitous Language

Key domain terms as used throughout the nit codebase.

## Core Data Model

| Term | Definition |
|---|---|
| **Diff** | The complete set of file changes being reviewed. Root data structure containing a list of `DiffFile`s. |
| **DiffFile** | A single file's changes within a diff. Tracks path, status, hunks, line counts, and viewed state. |
| **Hunk** | A contiguous block of changes within a file, corresponding to a unified diff hunk (delimited by `@@` headers). |

## Navigation

| Term | Definition |
|---|---|
| **Tab** | One of the three top-level views: `Diff`, `Commits`, or `PRs`. |
| **Focus** | Which pane currently receives keyboard input: `Sidebar` (file list) or `Diff` (content area). |

## View Modes

| Term | Definition |
|---|---|
| **ReviewMode** | What the user is currently reviewing: `WorkingTree` (uncommitted changes), `Commit` (a specific commit's diff), or `PullRequest` (a PR's diff). Commit and PR modes remember which tab to return to. |
| **DiffViewMode** | How diff content is rendered: `Unified` (single column with +/- prefixes) or `SideBySide` (two columns). |

## File Tree

| Term | Definition |
|---|---|
| **FileTree** | Hierarchical structure built from flat file paths. Folders are sorted before files. Single-child folder chains (e.g. `src/api/handlers/` when nothing else lives at those levels) are merged into one row labelled with the joined path. |
| **FlatEntry** | A single node in the flattened (renderable) tree. Has a depth level and is either a `Folder` or a `File`. Also carries `ancestor_has_next` and `is_last_sibling` for drawing tree connectors. |
| **Tree Connector** | The `├─`, `└─`, `│  ` glyphs drawn before each sidebar entry to make the parent-child hierarchy visible. Computed from each `FlatEntry`'s `ancestor_has_next` and `is_last_sibling`. |

## Side-by-Side Diff

| Term | Definition |
|---|---|
| **SplitRow** | One row in the side-by-side view. Variants: `HunkHeader`, `Context` (same on both sides), `Paired` (removed line matched with added line), `LeftOnly` (removal with no counterpart), `RightOnly` (addition with no counterpart). |
| **Pairing** | Matching a removed line with a corresponding added line so they appear on the same row for comparison. |
| **InlineSpan** | A byte range within a line marking a changed or unchanged segment. Used for character-level highlighting within paired lines. |
| **Inline Diff** | Character-level highlighting of what changed within a line, as opposed to just marking the whole line as added/removed. |

## Review Workflow

| Term | Definition |
|---|---|
| **Viewed** | A per-file flag indicating the reviewer has looked at the file. Shown as a checkmark in the sidebar. In WorkingTree mode, toggling viewed stages/unstages the file. |
| **Mark** | The action of setting a file as viewed and advancing to the next file. |

## Provider Abstraction

| Term | Definition |
|---|---|
| **RemoteProvider** | Trait defining the interface for fetching data from code review services. Methods: `fetch_diff`, `fetch_comments`, `fetch_metadata`, `fetch_pr_list`, `fetch_authenticated_user`. |
| **GitHubProvider** | Concrete `RemoteProvider` implementation for GitHub, using octocrab. Authenticates via `GITHUB_TOKEN` or `gh auth token`. |

## Filtering

| Term | Definition |
|---|---|
| **PrFilter** | Filter controls for the PR list. Options: open, draft, merged, closed, mine. |
