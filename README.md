# yourprompt

**Scores how well an AI coding agent can actually understand your prompt — against your actual repository — for exactly zero context tokens.**

> 프롬프트를 보내는 그 순간, "AI가 이 요청을 실행 가능하게 이해할 수 있는가"를 0 ~ 1000.0 점으로.
> 컨텍스트 윈도우는 단 1토큰도 쓰지 않습니다.

```
⟦ 742.3 ▓▓▓▓▓▓▓░░░ A- ⟧
```

## Why another prompt scorer?

Every prompt scorer on GitHub grades your prompt **in a vacuum** and never checks whether its
own score means anything. `yourprompt` does two things differently:

1. **Repository-conditioned grounding.** "Fix the login handler" is not a bad prompt in the
   abstract — it is bad *in a repo where `login` matches 37 symbols*. We index your codebase
   and measure whether each referent in your prompt resolves to exactly one thing.
2. **Validated against published benchmarks.** The score is checked against labelled
   ambiguity datasets (HumanEvalComm, ClarEval, Ambig-SWE, PartialOrderEval) and the
   numbers live in this README. See [Validation](#validation).

## Zero context cost

`UserPromptSubmit` hooks inject their stdout straight into Claude's context — that is why
LLM-backed prompt improvers cost you tokens. `yourprompt` writes **nothing** to stdout:

```
your prompt ─┬─▶ yp hook       stdout: ""  exit 0      → 0 tokens injected
             │                 side effect: session sidecar file
             └─▶ yp statusline reads sidecar → renders  → never enters context
```

## Try it

```console
$ yp score "fix the login handler"
  294.3 / 1000  F  ▓▓▓░░░░░░░

$ yp score "In src/auth/login.rs, change verify_token so it returns     Err(AuthError::Expired) instead of panicking when the token is expired.     Don't change the public signature of login().     tests/auth.rs::expired_token_is_rejected must pass."
  722.4 / 1000  B+  ▓▓▓▓▓▓▓░░░
```

Every sub-score prints with the reason it came out that way -- the model is
meant to be argued with, which it cannot be if it only prints a number.

## Try it without installing anything

**<https://liu-sky.github.io/yourprompt/>**

The whole scorer is compiled to WebAssembly and runs in the page — no server
and no model call. Drop your own source files on it and they are indexed in
your browser, so the grounding axis is about **your** codebase: whether the
things you name exist in it, and whether they point at one thing or forty.
Nothing you type or add is uploaded anywhere.

## Install

```bash
cargo install --git https://github.com/LiU-SKY/yourprompt yp-cli
yp install
```

`yp install` registers the `UserPromptSubmit` hook and the status line in your
Claude Code settings. It backs the file up first, and if you already have a
status line it **wraps** it rather than replacing it, so yours keeps working
with our segment appended. `--print-only` shows what it would write without
writing anything; `--uninstall` puts everything back.

Then, in any session:

```
[Opus] ~/proj main 42% ctx  ⟦ 742.3 ▓▓▓▓▓▓▓░░░ A- ▲ ⟧
```

`/score` explains the last prompt, axis by axis, and suggests one rewrite.

## In a browser

```bash
yp serve --repo .          # http://127.0.0.1:8787
```

Type a prompt and watch it scored as you go, with every component and the
reason it scored what it did. `--repo` indexes a codebase at startup so the
grounding axis is live.

Open a text file (or drop one on the box) to score a prompt you already
wrote, and download the result as a Markdown report with the full breakdown.
Both happen in the browser: the file is read locally and only its text is
scored, so the server keeps its three routes and gains no upload surface.

## Attachments are evidence, not claims

A prompt that carries the file it is about is a *better* prompt, and the score
says so. Material you paste or attach is judged on whether this repository
recognises it, not on how precisely it names things — because an attachment
makes no claims, it hands over evidence.

Getting that wrong was expensive: attaching the very source file a task was
about used to cost **70 points out of 1000**, because the file's 434 names were
averaged into the dozen the user had written by hand. Now attaching the right
file *raises* the score and attaching an unrelated one does not.

From a raw prompt — what the hook and the CLI see — a fenced block long enough
to have been pasted rather than typed is taken as an attachment. Short spans
stay inline: `verify_token` in the middle of a sentence is something you wrote.

## Gibberish scores like gibberish

Two components reward the *absence* of a defect, and text that is not language
has no defects to find. `asdfasefawefasf zxdf2wq4rq235wrsadgㅁㄴㅇㄹ` repeated a
few times used to score **356/1000 with clarity at full marks**. It now scores
24.

Recognising language needs no dictionary: Korean written in syllable blocks is
writing while bare jamo (ㅁㄴㅇㄹ) is a mashed keyboard; identifiers and paths are
judged against the repository instead; and Latin prose is checked against the
commonest few hundred words as a *ratio*, so technical vocabulary costs nothing
as long as ordinary words hold the sentence together.

The server is `std::net` and nothing else — no async runtime, no framework, no
dependency. It serves one page compiled into the binary and two JSON routes,
never touches the filesystem in response to a request, caps bodies and headers,
and binds to loopback unless you pass `--bind 0.0.0.0`. Scoring stays local:
no model is called and nothing is stored.

## What repository grounding looks like

The same three prompts, scored against this repository:

```
fix simplified_clarity_score in crates/yp-core/src/grounding.rs
  grounding  338.1 / 350   resolution 150.0/150  all 2 name(s) resolve to exactly one thing

fix the score handler
  grounding  212.5 / 350   resolution  37.5/150  0 of 1 names resolve; "score" could be any of 8

fix compute_paycheck in payroll.rs
  grounding  264.5 / 350   resolution  75.0/150  1 of 2 names resolve; "payroll.rs" is not in this repository
```

"Fix the login handler" is not vague in the abstract. It is vague *here*,
because `login` matches thirty-seven places and none of them is a definition.
That is a fact about your codebase, not a matter of taste, and it is the one
thing no other prompt scorer can tell you.

The index is built by `yp index`, which the session-start hook runs for you.
It is never built by the prompt hook: indexing takes seconds and nothing may
delay a prompt. Without an index the grounding axis reports itself
unavailable and the score is marked `~`.

## Status

🚧 Early development. M1 through M5 are done: the score is live in the status
line at zero context cost, grounded in your repository, in English and
Korean. The benchmark validation that turns the number from plausible into
*checked* has landed -- see [Validation](#validation). Absolute values remain
provisional; only the ordering is meaningful. See [milestones](#milestones).

## Validation

Every prompt scorer on GitHub asserts that its number means something. None of
them checks. `yp bench` does, against [HumanEvalComm](https://github.com/jie-jw-wu/human-eval-comm)
(Wu et al.): 164 HumanEval problems deliberately damaged into ambiguous,
inconsistent and incomplete variants, giving 771 pairs where we know which
side is worse.

| | Pairwise accuracy |
|---|---:|
| Defects that are **lexically reachable** (399 pairs) | **65.4%** |
| All 771 pairs, including the unreachable ones | 52.5% |

| Defect injected | Reachable | Pairs | Correct |
|---|:---:|---:|---:|
| incompleteness | yes | 164 | **80.5%** |
| ambiguity + incompleteness | yes | 71 | **78.9%** |
| ambiguity | yes | 164 | 42.7% |
| inconsistency | no | 163 | 24.5% (99 ties) |

**Why inconsistency is out of reach.** HumanEvalComm makes a problem
inconsistent by contradicting a docstring's worked example against its prose —
`truncate_number(3.5) -> 3` where the text says "return the decimal part".
Detecting that requires knowing the decimal part of 3.5 is 0.5. No dictionary,
density measure or corpus statistic gets there, which is why those pairs come
out as exact ties: to a lexical scorer the two prompts are identical. That is a
boundary of the LLM-free approach, and it is reported as one rather than
averaged away.

**These numbers are not flattering, and that is the point.** The first run of
this harness returned **45.3% — worse than a coin flip** — and the ablation
showed the clarity axis was actively hurting. Diagnosing one pair found three
real defects: hedges like "a certain" and "or another" were missing from the
lexicon entirely; because the damaged text was longer, the density
normalisation then read the extra words as dilution and *raised* the score; and
the `(e.g. …)` construct that created the ambiguity was being counted as a
worked example. Fixing those took the reachable figure to 64.7%. The bad first
number is [in the git history](../../commits/main) rather than tuned away
before publication.

Reproduce it yourself:

```bash
yp bench --ablation            # downloads and caches the dataset on first run
```

Full report: [bench/report.md](bench/report.md).

## Is the grounding axis real?

Axis A is the whole reason this is not another prompt scorer, so it gets its
own control — one that cannot be passed by accident. Score 270 real GitHub
issues from [SWE-bench Lite](https://huggingface.co/datasets/princeton-nlp/SWE-bench_Lite)
against the codebase each was actually filed against, then against unrelated
ones. A scorer that ignores the corpus returns the same number both times. So
does one that merely rewards long prompts.

```bash
yp bench --dataset swe --repos 6
```

| Comparison | Issues | Own repository scores higher | Mean margin |
|---|---:|---:|---:|
| versus the mean foreign repository | 270 | **87.4%** | +42.7 |
| versus the *best* foreign repository | 270 | **66.3%** | +24.1 |

| Repository | Issues | Own scores higher |
|---|---:|---:|
| scikit-learn | 23 | 100.0% |
| matplotlib | 23 | 95.7% |
| sympy | 77 | 90.9% |
| django | 114 | 88.6% |
| pytest | 17 | 76.5% |
| sphinx | 16 | 43.8% |

**The first run of this control failed outright: 47.0%, below chance, mean
margin −0.4.** Swapping the repository barely moved the score. Three defects
were behind it, and all three are the kind only a measurement finds:

- **The specificity sub-score was inverted.** It used the Simplified Clarity
  Score, which smooths terms the collection has never seen and so reads them as
  maximally distinctive. Scoring against the *wrong* repository makes nearly
  every term unseen — so the further a prompt was from a codebase, the better it
  scored. A sympy issue scored higher specificity against matplotlib than
  against sympy. Replaced with average IDF over the names a prompt uses, with
  unfound names folded in as zero.
- **Pasted code was thrown away.** A snippet became one referent, looked up
  whole, and never resolved — discarding the most groundable content a prompt
  can contain. Snippets are now read for the names inside them.
- **A bare lowercase word inside backticks was treated as prose.** `ccode` and
  `sinc` carry no underscore or camel hump, so the tokenizer filed them as
  English. Inside a snippet they are names.

Two further defects came out of the per-issue dump (`yp bench --dataset swe
--dump …`), taking it from 80.4% to 87.4%:

- **Resolution was a flat average.** A GitHub issue names 90–190 things, most
  of them ordinary words that happen to exist in the codebase, so the one name
  that actually pins the work down counted for about one part in a hundred and
  fifty. It is now weighted by how much each name narrows things down, and
  naming something that does not exist carries the weight it would have earned
  had it been unique.
- **Informativeness was measured wrongly, in two ways.** It used document
  frequency, which inverts for any project whose vocabulary is its own subject:
  `sphinx` appears in 1286 of sphinx's 1336 files, so sphinx looked *less*
  informed about its own vocabulary than a project that merely imports it. And
  it normalised by repository size, so a name was worth more simply for living
  in a bigger project. It now counts definition sites against a fixed
  reference — what costs an agent time is the absolute number of places it has
  to look.

### What still does not work

**sphinx, at 43.8%, is still below chance.** The cause is now understood and is
partly real: every one of these projects builds its documentation with sphinx,
so sphinx's vocabulary genuinely lives in all of them — `toctree` is in 244 of
sphinx's files and in 8–44 files of each of the others. Counting definitions
rather than mentions recovered part of it (sphinx defines `toctree` fifteen
times, the others never), but a sphinx issue really is partly grounded in any
project that uses sphinx.

**The gold-patch test produced nothing**, and the reason is a flaw in the test:
**99% of issues already contain at least one name that resolves to exactly one
thing**, so both groups are saturated and the label has no headroom. It is also
asking a different question than it appears to — naming the file a maintainer
eventually edited is not the same as being well grounded, since an issue can
point precisely at a symptom whose fix belongs elsewhere. Reported rather than
dropped, because it was offered as evidence and delivered none.

The full report is at [bench/grounding.md](bench/grounding.md). The failing
measurements are in the commit history, not edited out of it.

### Caveats worth stating

- HumanEvalComm's prompts are function stubs with docstrings, not the
  imperative requests a coding agent actually receives. The *ordering* is
  meaningful; the absolute scores on this dataset are not.
- Grounding is inactive throughout, since these prompts have no repository. That
  axis is measured separately — see [Is the grounding axis real?](#is-the-grounding-axis-real).
- Scoring parameters remain reasoned defaults rather than fitted ones.

## Milestones

- [x] **M1** — core scoring engine (B/C/D axes), `yp score`
- [x] **M2** — zero-context integration: `yp hook`, `yp statusline`, `yp install`
- [x] **M3** — A axis: repository symbol index, `resolve@1`, SCS
- [x] **M4** — Korean support (shipped with the lexicons from the start)
- [x] **M5** — `yp bench`, ablation table, published numbers
- [x] **M6** — validate the grounding axis on real repositories (SWE-bench Lite)

## Prior art & references

The scoring model is grounded in published research rather than invented from scratch:

- Femmer et al., *Rapid quality assurance with Requirements Smells*, JSS 2017 — ISO/IEC/IEEE
  29148-derived defect taxonomy detectable without an LLM.
- Hauff et al., *A survey of pre-retrieval query performance predictors*, CIKM 2008 —
  predicting query success from corpus statistics before execution.
- Wu et al., *HumanEvalComm* — ambiguity / inconsistency / incompleteness taxonomy for code prompts.
- *PartialOrderEval* (arXiv:2508.03678) — explicit I/O specs, edge cases and stepwise
  breakdown as the drivers of prompt-detail gains.
- *MutaGReP* (arXiv:2502.15872) — execution-free repository-grounded plan feasibility.

## License

MIT
