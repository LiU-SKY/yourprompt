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

🚧 Early development. M1 through M4 are done: the score is live in the status
line at zero context cost, grounded in your repository, in English and
Korean. The benchmark validation that turns the number from plausible into
*checked* is M5, and until it lands the absolute values are provisional --
only the ordering is meaningful. See [milestones](#milestones).

## Validation

_Benchmark numbers land here once `yp bench` is wired up (M5)._

## Milestones

- [x] **M1** — core scoring engine (B/C/D axes), `yp score`
- [x] **M2** — zero-context integration: `yp hook`, `yp statusline`, `yp install`
- [x] **M3** — A axis: repository symbol index, `resolve@1`, SCS
- [x] **M4** — Korean support (shipped with the lexicons from the start)
- [ ] **M5** — `yp bench`, ablation table, marketplace listing

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
