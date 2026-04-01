# nit

A terminal-based code review tool built in Rust. Name comes from "nitpick" — the small, careful comments that make code better.

A GitHub-like "Files changed" experience in the terminal — structured, navigable, keyboard-driven.

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

## Building

```bash
cargo build --release
```

## License

MIT
