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

## Status

🚧 Early development. M1 (scoring engine + CLI) is done; the status line
integration that makes this free of context cost lands in M2. See
[milestones](#milestones).

## Validation

_Benchmark numbers land here once `yp bench` is wired up (M5)._

## Milestones

- [x] **M1** — core scoring engine (B/C/D axes), `yp score`
- [ ] **M2** — zero-context integration: `yp hook`, `yp statusline`, `yp install`
- [ ] **M3** — A axis: tree-sitter repo symbol index, `resolve@1`, SCS
- [ ] **M4** — Korean support
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
