# nit

A terminal-based code review tool built in Rust. Name comes from "nitpick" — the small, careful comments that make code better.

A GitHub-like "Files changed" experience in the terminal — structured, navigable, keyboard-driven.

This is first and foremost a diff/review tool. Although there are some git functionality, they are  mostly there for convenience. There are similar tools out there that are more feature rich and
probably better for most people, but they did not really work for me, so I built my own tool to work exactly as I wanted it to.

## Features

- **File tree sidebar** with collapsible folders
- **Switch between unified and side-by-side diff views** with syntax highlighting and character-level inline diffs
- **Search** within diffs with match navigation
- **GitHub PR browser** - browse and review the diff of any PR (includes state filters)
- **Commit log browser** — browse and review the diff of any commit
- **Git operations** — stage files, commit, switch branches, pull, push
- **Configurable** — syntax themes, full color customization

## Usage

```bash
nit                    # uncommitted changes
nit abc123             # single commit
nit abc123..def456     # commit range
nit '#42'              # GitHub PR (current repo)
nit owner/repo#42      # GitHub PR (any repo)
```

GitHub PR mode requires a token: set `GITHUB_TOKEN` or have `gh` CLI authenticated.

## Keybindings

Press `?` in-app to see the full help overlay.

### Global

| Key | Action |
|---|---|
| `1` / `2` / `3` | Switch tab (Diff / Commits / PRs) |
| `q` | Quit |
| `?` | Toggle help |
| `gg` / `G` | Jump to top / bottom |

### Diff — Sidebar

| Key | Action |
|---|---|
| `j` / `k` | Navigate files |
| `l` / `Enter` | Open file / toggle folder |
| `Tab` | Focus diff panel |

### Diff — Panel

| Key | Action |
|---|---|
| `j` / `k` | Scroll up / down |
| `Ctrl+d` / `Ctrl+u` | Half-page down / up |
| `Ctrl+f` / `Ctrl+b` | Full-page down / up |
| `PageDown` / `PageUp` | Full-page down / up |
| `J` / `K` | Next / prev hunk |
| `Ctrl+n` / `Ctrl+p` | Next / prev file |
| `]` / `[` | Next / prev file (alias) |
| `h` | Focus sidebar |

### Diff — Shared

| Key | Action |
|---|---|
| `Space` | Stage file / folder |
| `V` | Mark viewed & next |
| `/` then `Enter` | Search in file |
| `n` / `N` | Next / prev match |
| `s` | Toggle unified / split view |
| `Ctrl+e` | Open in `$EDITOR` |
| `Esc` | Clear search or go back |

### Local repo (default mode)

| Key | Action |
|---|---|
| `c` | Commit staged changes |
| `b` | Switch branch |
| `p` / `P` | Git pull / push |

### Commits / PRs

| Key | Action |
|---|---|
| `j` / `k` | Navigate list |
| `Ctrl+d` / `Ctrl+u` | Half-page down / up |
| `Ctrl+f` / `Ctrl+b` | Full-page down / up |
| `PageDown` / `PageUp` | Full-page down / up |
| `Enter` | View diff |
| `f` | Filter PRs (PRs tab) |
| `Esc` | Return to list |

## Configuration

Config file: `~/.config/nit/nit.toml`

```toml
[theme]
# Bundled themes: base16-ocean.dark, base16-eighties.dark,
#                 InspiredGitHub, Solarized (dark), Solarized (light)
syntax = "base16-ocean.dark"
```

#### Custom syntax themes

nit supports `.tmTheme` (TextMate) and `.sublime-color-scheme` files. There are two ways to use them:

**Drop-in directory** — place theme files in `~/.config/nit/themes/` and reference by name:

```toml
[theme]
syntax = "Nord"  # matches ~/.config/nit/themes/Nord.sublime-color-scheme
```

**Explicit path** — point directly at a file anywhere on disk. This is useful if you keep a shared themes directory across tools:

```toml
[theme]
syntax_file = "~/.config/themes/Nord.sublime-color-scheme"
```

`syntax_file` takes priority over `syntax` when both are set.

### Colors

By default, nit uses your terminal's ANSI colors, so it will inherit whatever theme you have configured. You can override individual colors with hex values in the `[colors]` section:

| Field | Default | Used for |
|---|---|---|
| `bg` | terminal default | Background |
| `border_focused` | cyan | Focused panel borders |
| `border_unfocused` | dark gray | Unfocused panel borders |
| `fg` | white | Normal text, file names |
| `fg_muted` | dark gray | Hints, stats, hunk headers |
| `fg_selected` | cyan | Selected items |
| `fg_added` | green | Added lines, viewed markers |
| `fg_removed` | red | Removed lines |
| `fg_accent` | yellow | Folders, search indicator |
| `fg_info` | magenta | Branch name |
| `bg_added` | `#1E3C1E` | Added line background |
| `bg_removed` | `#3C1E1E` | Removed line background |
| `bg_inline_added` | `#285028` | Inline added highlight (split view) |
| `bg_inline_removed` | `#502828` | Inline removed highlight (split view) |
| `bg_split_empty` | `#19191E` | Empty side background (split view) |
| `bg_search_match` | `#78641E` | Search match highlight |
| `bg_search_current` | `#B48C14` | Current search match |
| `bg_selected` | `#283246` | Selected item highlight background |

#### Nord theme

If your terminal doesn't use a dark theme, or you want consistent colors regardless of terminal, here's the complete Nord-based config I use:

```toml
[colors]
bg = "#2E3440"
border_focused = "#88C0D0"
border_unfocused = "#4C566A"
fg = "#E5E9F0"
fg_muted = "#4C566A"
fg_selected = "#88C0D0"
fg_added = "#A3BE8C"
fg_removed = "#BF616A"
fg_accent = "#EBCB8B"
fg_info = "#B48EAD"
bg_added = "#1E3C1E"
bg_removed = "#3C1E1E"
bg_inline_added = "#285028"
bg_inline_removed = "#502828"
bg_split_empty = "#1E222A"
bg_search_match = "#78641E"
bg_search_current = "#B48C14"
bg_selected = "#3B4252"
```

## Building

```bash
cargo build --release
```

## License

MIT
