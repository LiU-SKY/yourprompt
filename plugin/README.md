# yourprompt plugin for Claude Code

Registers the `UserPromptSubmit` hook that scores each prompt, the
`SessionStart` hook that indexes the repository in the background, and the
`/score` command that explains the last score.

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

## Install the plugin

The repository is its own plugin marketplace, called `yourprompt`. In Claude
Code:

```
/plugin marketplace add LiU-SKY/yourprompt
/plugin install yourprompt@yourprompt
```

Or from your shell:

```bash
claude plugin marketplace add LiU-SKY/yourprompt
claude plugin install yourprompt@yourprompt
```

Start a new session afterwards. Without `yp` on your `PATH` the hooks fail to
run, and nothing is scored.

## The plugin or `yp install`, not both

Both register the same `UserPromptSubmit` hook, and Claude Code runs a
plugin's copy of a hook as well as the one in your settings. With both, every
prompt is scored twice. Use one of them.

**`yp install`** needs no plugin at all. It registers the hooks in your own
settings, adds `/score` to your commands, and adds the status line, which a
plugin cannot ship: a plugin's settings only honour `agent` and
`subagentStatusLine`. A status line you already have keeps working, with the
score appended to it.

```bash
yp install                # backs up your settings, keeps any status line you have
yp install --print-only   # see what it would write, change nothing
yp install --uninstall    # undo it, restoring the status line it wrapped
```

**The plugin** registers the hooks and `/score`. The status line is then
yours to add, in `~/.claude/settings.json`:

```json
{
  "statusLine": { "type": "command", "command": "yp statusline" }
}
```

If you already have a status line, use `yp install` instead: it keeps yours.

## Why the hook prints nothing

Claude Code injects a `UserPromptSubmit` hook's stdout straight into the
model's context. `yp hook` therefore writes nothing at all: it stores the
score in a file, and `yp statusline` reads it back. Status line output never
reaches the model, so the whole thing costs zero tokens. `/score` is set so
that Claude does not invoke it on its own, which keeps its description out of
the context too.

## Why the index is built in the background

Claude Code holds a session's first prompt until its `SessionStart` hooks
have finished, and the first index of a large repository takes a few seconds
to build: 3.8 s for 7,030 files nobody had opened before, on Windows, where
the antivirus scans each file the first time it is read. Nothing needs the
index straight away: until it exists, prompts are scored without the
grounding axis, and the status line marks the score with `~`. So the hook
runs with `"async": true`, and your first prompt goes straight through.
