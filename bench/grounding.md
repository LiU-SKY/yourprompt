# yourprompt benchmark -- grounding

Dataset: [SWE-bench Lite](https://huggingface.co/datasets/princeton-nlp/SWE-bench_Lite), 270 real GitHub issues across 6 repositories.

Axis A claims to read your repository. This checks that claim the only way that cannot be satisfied by accident: score each issue against the codebase it was actually filed against, then against unrelated ones. A scorer that ignores the corpus returns the same number both times. So does one that merely rewards long prompts. Only genuine resolution against a repository's vocabulary can tell them apart.

## Cross-repository control

| Comparison | Issues | Own repository scores higher | Mean margin |
|---|---:|---:|---:|
| versus the mean foreign repository | 270 | **47.0%** | -0.4 |
| versus the *best* foreign repository | 270 | **22.6%** | -8.6 |

| Repository | Issues | Own scores higher | Mean margin |
|---|---:|---:|---:|
| django/django | 114 | 33.3% | -2.5 |
| matplotlib/matplotlib | 23 | 87.0% | 6.3 |
| pytest-dev/pytest | 17 | 23.5% | -8.8 |
| scikit-learn/scikit-learn | 23 | 60.9% | 3.0 |
| sphinx-doc/sphinx | 16 | 6.2% | -12.4 |
| sympy/sympy | 77 | 64.9% | 4.1 |

## Gold-patch test

The patch names the files that actually had to change. An issue naming one of them has told the agent where to go; one that does not has left it to search. Mean `resolution` score, out of 150:

| Issue names a file the fix touched | Issues | Mean resolution |
|---|---:|---:|
| yes | 60 | **41.1** |
| no | 210 | 39.7 |

Separation: **+1.4** points.

## Caveats

- One index per repository, built at the base commit of that repository's first instance. Repositories evolve, but their vocabulary barely moves between nearby commits, and this turns three hundred downloads into a dozen. It is an approximation.
- SWE-bench issues are written by maintainers and users to be filed, not typed at a coding agent. They are far longer than a typical prompt.
- The cross-repository control shows the axis is repository-specific. It does not show that the resulting *number* predicts whether an agent will succeed; that would need agent runs, not scores.
