# yourprompt plugin for Claude Code

Registers the `UserPromptSubmit` hook that scores each prompt, and the
`/score` command that explains the last one.

## Requirements

The `yp` binary must be on your `PATH`, because the plugin's hooks call it by
name. Install it first.

macOS and Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/LiU-SKY/yourprompt/main/install.sh | sh
```

Windows, in PowerShell:

```powershell
irm https://raw.githubusercontent.com/LiU-SKY/yourprompt/main/install.ps1 | iex
```

Both download a prebuilt binary from the latest release and check its SHA-256.
The Windows script adds its directory to your user PATH; the macOS and Linux
one installs to `~/.local/bin` and tells you if that is not on your `PATH` yet.

Or from source, with Rust 1.85 or newer:

```bash
cargo install --git https://github.com/LiU-SKY/yourprompt yp-cli
```

## The status line

Claude Code plugins can ship hooks but **cannot** ship a `statusLine` - a
plugin's settings only honour `agent` and `subagentStatusLine`. So the piece
that actually displays the score has to go in your own settings:

```bash
yp install          # backs up your settings, wraps any status line you have
yp install --print-only   # see what it would write, change nothing
yp install --uninstall    # undo, restoring a wrapped status line
```

`yp install` also registers the hook, so if you use it you do not need this
plugin at all. The plugin exists for people who install everything through
the marketplace and only want to add the status line by hand.

## Why the hook prints nothing

Claude Code injects a `UserPromptSubmit` hook's stdout straight into the
model's context. `yp hook` therefore writes nothing at all: it stores the
score in a file, and `yp statusline` reads it back. Status line output never
reaches the model, so the whole thing costs zero tokens.
