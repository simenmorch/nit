# nit

A terminal-based code review tool built in Rust. Name comes from "nitpick" — the small, careful comments that make code better.

A GitHub-like "Files changed" experience in the terminal — structured, navigable, keyboard-driven.

This is a dead simple tool. There are similar diff tools out there that are more feature rich and probably better for most people, but none of them worked for me. 
I therefore built my own tool to fit my needs.

## Usage

```bash
nit                    # uncommitted changes
nit abc123             # single commit
nit abc123..def456     # commit range
nit '#42'              # GitHub PR (current repo)
nit owner/repo#42      # GitHub PR (any repo)
```

## Keybindings

### Global

| Key       | Action                |
|-----------|-----------------------|
| `q`       | Quit                  |
| `Tab`     | Toggle sidebar        |
| `v`       | Toggle file as viewed |
| `/`       | Search in diff        |
| `n` / `N` | Next / previous match |
| `gg`      | Jump to top           |
| `G`       | Jump to bottom        |

### Sidebar (file list)

| Key            | Action              |
|----------------|----------------------|
| `j` / `Down`   | Next file            |
| `k` / `Up`     | Previous file        |
| `l` / `Enter`  | Focus diff view      |

### Diff view

| Key          | Action              |
|--------------|----------------------|
| `j` / `Down` | Scroll down          |
| `k` / `Up`   | Scroll up            |
| `h`          | Focus sidebar        |
| `Esc`        | Clear search / focus sidebar |

## Configuration

Config file: `~/.config/nit/nit.toml`

```toml
[theme]
# Bundled themes: base16-ocean.dark, base16-eighties.dark,
#                 InspiredGitHub, Solarized (dark), Solarized (light)
syntax = "base16-ocean.dark"

# Or load a custom .tmTheme file:
# syntax_file = "/path/to/Nord.tmTheme"
```

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
| `bg_search_match` | `#78641E` | Search match highlight |
| `bg_search_current` | `#B48C14` | Current search match |

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
bg_search_match = "#78641E"
bg_search_current = "#B48C14"
```

## Building

```bash
cargo build --release
```

## License

MIT
