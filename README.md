<h1 align="center">yourprompt</h1>

<p align="center">
  A prompt score for Claude Code that costs zero context tokens.<br>
  Grounded in your repository. Deterministic. No model call.
</p>

<p align="center">
  <a href="https://liu-sky.github.io/yourprompt/"><b>Try it in the browser</b></a> ·
  <a href="#install">Install</a> ·
  <a href="#how-the-score-is-built">How the score is built</a> ·
  <a href="#benchmarks">Benchmarks</a>
</p>

<p align="center">
  <a href="https://github.com/LiU-SKY/yourprompt/actions/workflows/ci.yml"><img src="https://github.com/LiU-SKY/yourprompt/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/LiU-SKY/yourprompt/actions/workflows/pages.yml"><img src="https://github.com/LiU-SKY/yourprompt/actions/workflows/pages.yml/badge.svg" alt="Pages"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT"></a>
  <img src="https://img.shields.io/badge/rust-1.85%2B-orange.svg" alt="Rust 1.85+">
  <img src="https://img.shields.io/badge/lang-EN%20%7C%20KO-lightgrey.svg" alt="English and Korean">
</p>

```
[Opus] ~/proj main 42% ctx  ⟦ 860.3 ▓▓▓▓▓▓▓▓▓░ A+ ▲ ⟧
```

<p align="center">
  <img src="docs/og.png" width="720"
       alt="&quot;fix the bug&quot; scores 305.4, an F. A prompt that names the file, the function and when it is done scores 799.8, an A-.">
</p>

Every prompt you send gets a 0 to 1000 score in the status line, before the
model answers. The score says how well a coding agent can act on the prompt
*in this repository*: whether the things you name exist, whether they point at
one place or forty, and whether you said what done looks like.

- **Zero tokens.** The hook prints nothing. The status line never enters the
  model's context. Your `/context` count is the same with or without it.
- **Grounded.** `yp index` builds a symbol index of your repo. "Fix the login
  handler" scores differently in a repo where `login` is one function and in
  one where it matches 37 places.
- **No model call.** Dictionaries, corpus statistics and a tokenizer. Same
  input, same score, every time. Nothing leaves your machine.
- **Explains itself.** `yp explain` prints every component with the reason it
  got that number. `/score`, which `yp install` adds, hands that breakdown to
  Claude for the component that cost the most and one rewrite.
- **Shows it in place.** In the web UI the prompt is coloured as you type:
  green where it earned points, yellow where it is unclear, red where it lost
  them, with a chip per word saying why.
- **Grades tasks, not chatter.** Slash commands and short follow-ups like
  "yes", "continue" or "계속해" are not graded, so the status line keeps your
  last task's score instead of flashing an F.
- **English and Korean.** A prompt in Spanish, Japanese, Russian or 25 other
  languages says "not graded" rather than failing. One of two or three words
  can still slip through and be graded.

## Try it

No install: **<https://liu-sky.github.io/yourprompt/>**. The scorer is compiled
to WebAssembly. Drop your own source files on the page and they are indexed
locally, so the grounding axis is about your code. Nothing is uploaded.

With no files loaded, names cannot be checked, and the page says so next to
the grade: a prompt about a made-up `src/banana.rs` scores the same 799.8 as
the real one. Load the six-file sample repo and the real prompt rises to 860.3
while the made-up one falls to 563.8.

Or from the command line, here in a directory with no index, which the `~`
marks:

```console
$ yp score --oneline "fix the login handler"
294.3 ▓▓▓░░░░░░░ F ~

$ yp score --oneline "In src/auth/login.rs, change verify_token so it returns Err(AuthError::Expired) instead of panicking on an expired token. Keep the public signature of login(). tests/auth.rs::expired_token_is_rejected must pass."
798.3 ▓▓▓▓▓▓▓▓░░ A- ~

$ yp score --oneline "continue"
not graded: a follow-up like this leans on the conversation
```

Without `--oneline` it prints every component and why it got that number.

## Install

macOS and Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/LiU-SKY/yourprompt/main/install.sh | sh
```

Windows, in PowerShell:

```powershell
irm https://raw.githubusercontent.com/LiU-SKY/yourprompt/main/install.ps1 | iex
```

Each script downloads the build for your machine from the latest release,
checks it against its published SHA-256, and puts `yp` in `~/.local/bin`. On
Windows it goes in `%LOCALAPPDATA%\Programs\yourprompt`, which is added to your
user PATH. No sudo, no administrator. `curl ... | YP_VERSION=v0.1.1 sh` (or
`$env:YP_VERSION` in PowerShell) pins a release, and `YP_INSTALL_DIR` picks the
directory.

The same builds are on the
[Releases page](https://github.com/LiU-SKY/yourprompt/releases): Linux (static,
x86_64 and arm64), macOS (Intel and Apple silicon) and Windows (x64 and arm64),
each with a `.sha256` file that `sha256sum -c` checks.

Or build from source, with Rust 1.85 or newer:

```bash
cargo install --git https://github.com/LiU-SKY/yourprompt yp-cli
```

Then:

```bash
yp install
```

`yp install` registers the hooks and the status line in your Claude Code
settings and adds the `/score` command. The first time it changes your
settings it backs them up. If you already have a status line it keeps it, runs
it, and appends the score segment. The hooks need Claude Code 2.1.139 or newer.

```bash
yp install --print-only   # show the change, write nothing
yp install --uninstall    # remove what it added, restoring a status line it wrapped
```

There is also a Claude Code plugin under [`plugin/`](plugin/): the same hooks
and `/score`, but no status line, which a plugin cannot ship. Use the plugin or
`yp install`, not both: each registers the hook, and Claude Code would run it
twice.

## How it stays at zero tokens

Claude Code injects a `UserPromptSubmit` hook's stdout into the model's
context. That is why LLM-based prompt improvers cost tokens. Status line output
never reaches the model. So:

```
prompt ─┬─▶ yp hook        stdout: ""   exit 0     nothing injected
        │                  writes ~/.claude/yourprompt/sessions/<id>.json
        └─▶ yp statusline  reads the file, renders the bar
```

The hook never blocks and never indexes. If anything goes wrong it exits 0
silently. Indexing runs from the session-start hook, or by hand with `yp index`.

## How the score is built

Four axes, 1000 points total. Every count goes through a saturating function,
and each axis has a cap, so no single trick can max the score.

| Axis | Points | What it measures |
|---|---:|---|
| **Grounding** | 350 | Do the names in the prompt resolve to exactly one thing in this repo? How specific is the vocabulary to this codebase? Are there pronouns with no antecedent ("it", "that one", "그거")? |
| **Actionability** | 250 | One clear command verb and goal. Explicit inputs and outputs. A stated acceptance criterion ("tests pass", "must return X"). |
| **Clarity** | 250 | Starts full, loses points per ambiguity smell: subjective terms, comparatives with no baseline, hedges, vague adverbs, passive voice, escape clauses, contradictions. |
| **Context** | 150 | Scope limits ("don't touch X"), examples, code blocks, lists, and length in a reasonable range. |

Without an index the grounding axis is off and the other three are
renormalised to 1000. The status line marks this with `~`.

The smell list comes from Femmer et al., *Rapid quality assurance with
Requirements Smells* (JSS 2017), which derives it from ISO/IEC/IEEE 29148.
The specificity measure comes from pre-retrieval query performance prediction
(Hauff et al., CIKM 2008). See [Prior art](#prior-art).

### Grounding, on this repository

```
fix dangling_deixis in crates/yp-core/src/grounding.rs
  grounding  345.7 / 350   resolution 150.0/150  all 2 name(s) resolve to exactly one thing

fix the score handler
  grounding   80.0 / 350   resolution   0.0/150  names nothing that exists in this repository

fix compute_paycheck in payroll.rs
  grounding  197.0 / 350   resolution  26.5/150  0 of 2 names resolve; "payroll.rs" is not in this repository
```

`compute_paycheck` does appear here, in three files of tests and docs that use
it as an example, which is where the 26.5 comes from. "score" is in most files,
so it counts as vocabulary rather than as a name.

### Things that used to score well and no longer do

Two components reward the absence of a defect, and text that is not language
has no defects. So 테스트 typed ten times, a screen of digits, and 아 held down
for eight lines used to score 357 to 477, more than a vague request. Each is
now under 100. The rule: credit for the absence of a defect is scaled by
legibility, lexical variety, and whether the text asks for anything at all.
A greeting has no vague words about the work because it contains no work. An
English verb typed ten times still reads as one request: "test" ten times
scores 262.9, an F.

Attached files are treated as evidence rather than as claims. Pasting a file
used to cost 70 points, because its 434 identifiers were averaged into the
dozen you typed. A pasted file now counts as context, but the score cannot
tell the file a task is about from any other. With the demo's sample repo
loaded, pasting `login.rs` into the prompt about `src/auth/login.rs` takes it
from 860.3 to 859.3, while the unrelated `tests/auth.rs` lifts it to 882.7.

The same goes for a pasted error. "fix this" over a stack trace in a code
fence scored 278.3, because "this" was read as pointing at nothing. The trace
is what it points at: the same prompt now scores 536.2. A fence of keyboard
mash gets only the flat credit any code block earns, and stays at 278.3.

A direct question is a request. "Why does verify_token panic on expired
tokens?" scored 155.3 for having no action verb; it now scores 511.1, while
"why is it broken again?" stays at 65.9.

A forbidden verb is a boundary, not the task. "Don't change anything. Tests
must still pass." scored 673.4, with "change" read as the thing to do. It is
now 519.6, and "Done means: the test suite passes." counts as a finish line.

## Benchmarks

The score is checked against published datasets, and the first results were
bad. They are in the commit history. `yp bench` reproduces the HumanEvalComm
numbers, and `yp bench --dataset swe` the SWE-bench Lite ones, after
downloading six repositories. The Korean fixture is `bench/korean.txt`, and
`cargo test -p yp-core the_korean_fixture_holds_its_ordering` checks that it
stays above 90%.

### Ordering: HumanEvalComm

164 HumanEval problems, each damaged into ambiguous, inconsistent and
incomplete variants. 771 pairs where the worse side is known. Beside the score
is the baseline to beat: call whichever prompt is longer the original.

| | Pairs | Original scores higher | Longer prompt wins |
|---|---:|---:|---:|
| Defects visible in the text | 399 | 66.2% | 72.7% |
| All pairs | 771 | 53.6% | 59.0% |

| Defect | Pairs | Correct | Longer prompt wins |
|---|---:|---:|---:|
| incompleteness | 164 | 83.5% | 100.0% |
| ambiguity + incompleteness | 71 | 77.5% | 100.0% |
| ambiguity | 164 | 43.9% | 33.5% |
| inconsistency | 163 | 24.5%, 98 ties | 35.6% |

Length beats the score overall, and that is worth saying first. HumanEvalComm
makes a prompt incomplete by cutting text out, so the shorter side is the
damaged one in all 164 incomplete pairs: no scorer can do better than length
there, and this one does not. On vague wording, where length cannot help, the
score is right 43.9% of the time: below a coin flip, but ahead of length's
33.5%. The grounding benchmark below is immune to length altogether, because
both sides of every comparison are the same text.

Inconsistency is out of reach for a lexical scorer. HumanEvalComm makes a
docstring inconsistent by contradicting its worked example, for instance
`truncate_number(3.5) -> 3` against prose that asks for the decimal part.
Catching that means evaluating the example. Those pairs come out as ties.

The first run returned 45.3%. The ablation showed the clarity axis making
things worse. Three bugs: hedges like "a certain" were missing from the
lexicon; length normalisation read the damaged, longer text as diluted and
raised its score; and the `(e.g. ...)` that introduced the ambiguity was
counted as an example.

### Grounding: SWE-bench Lite

270 of SWE-bench Lite's 300 issues, the ones from its six largest
repositories. Each is scored against the repository it was filed on and then
against the five others. A scorer that ignores the corpus returns the same
number both times.

| Comparison | Issues | Own repo scores higher | Mean margin |
|---|---:|---:|---:|
| vs the mean foreign repo | 270 | **88.5%** | +47.3 |
| vs the best foreign repo | 270 | 71.5% | +28.5 |

| Repository | Issues | Own repo higher |
|---|---:|---:|
| scikit-learn | 23 | 100.0% |
| matplotlib | 23 | 91.3% |
| sympy | 77 | 90.9% |
| django | 114 | 90.4% |
| pytest | 17 | 76.5% |
| sphinx | 16 | 56.2% |

The first run was 47.0%, below chance. The specificity measure was inverted:
it smoothed unseen terms as maximally distinctive, so the further a prompt was
from a codebase the better it scored. Pasted code was looked up as one token.
Resolution was a flat average over 100+ names, so the one name that mattered
counted for nothing. The full report, with its caveats and a test that came
back empty, is in [bench/grounding.md](bench/grounding.md).

sphinx is barely above chance, and part of that is real: every project here
builds its docs with sphinx, so sphinx's vocabulary lives in all of them.

### Korean

A 32-prompt fixture in five tiers, from a held-down key to a full request.
Scored without an index, as CI scores it, cross-tier ordering went from 74.7%
to **97.5%** (385 of 395 pairs), and the English benchmarks did not drop.
Against this repository's own index it is 98.7%. See
[bench/korean.md](bench/korean.md).

### Caveats

- HumanEvalComm prompts are function stubs, not requests to an agent. The
  ordering means something; the absolute values on that dataset do not.
- Scoring parameters are reasoned defaults, not fitted ones.
- It grades whether an agent can act on a prompt, not whether the request is
  wise. A precise "delete every file and force-push" scores well.
- Word-level rules can be gamed: a sentence built from the cue words scores
  better than it deserves, most of all with no files to check names against.
- The per-component explanation sentences are English only for now.

## How this was built

It started with a question: what actually makes a prompt good? I wanted to
see a score for a prompt before I sent it to the AI, not after the answer came
back wrong.

Claude Code wrote most of the code with me, and the commits carry its
trailer. The failures along the way, like the first benchmark runs that came
out below chance, are in the history as well.

## Local web UI

```bash
yp serve --repo .          # http://127.0.0.1:8787
```

The same scorer in a simpler sibling of the hosted page, with the repository
indexed at startup. The server is `std::net` only: one embedded page, two JSON
routes, loopback unless you pass `--bind`. Prompts are never stored; the index
is written where `yp index` writes it.

## Prior art

- Femmer et al., *Rapid quality assurance with Requirements Smells*, JSS 2017.
- Hauff et al., *A survey of pre-retrieval query performance predictors*, CIKM 2008.
- Wu and Fard, *HumanEvalComm: Benchmarking the Communication Competence of
  Code Generation for LLMs and LLM Agent*, arXiv:2406.00215.
- *PartialOrderEval*, arXiv:2508.03678.
- *MutaGReP*, arXiv:2502.15872.
- SWE-bench Lite, princeton-nlp.

## License

MIT
