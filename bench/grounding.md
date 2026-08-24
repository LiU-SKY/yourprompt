# yourprompt benchmark -- grounding

Dataset: [SWE-bench Lite](https://huggingface.co/datasets/princeton-nlp/SWE-bench_Lite), 270 real GitHub issues across 6 repositories.

Axis A claims to read your repository. This checks that claim the only way that cannot be satisfied by accident: score each issue against the codebase it was actually filed against, then against unrelated ones. A scorer that ignores the corpus returns the same number both times. So does one that merely rewards long prompts. Only genuine resolution against a repository's vocabulary can tell them apart.

## Cross-repository control

| Comparison | Issues | Own repository scores higher | Mean margin |
|---|---:|---:|---:|
| versus the mean foreign repository | 270 | **87.4%** | 42.7 |
| versus the *best* foreign repository | 270 | **69.6%** | 25.4 |

| Repository | Issues | Own scores higher | Mean margin |
|---|---:|---:|---:|
| django/django | 114 | 89.5% | 61.4 |
| matplotlib/matplotlib | 23 | 95.7% | 32.5 |
| pytest-dev/pytest | 17 | 76.5% | 13.0 |
| scikit-learn/scikit-learn | 23 | 100.0% | 60.2 |
| sphinx-doc/sphinx | 16 | 56.2% | 5.6 |
| sympy/sympy | 77 | 87.0% | 27.1 |

## Gold-patch test (a negative result)

The patch names the files that actually had to change. The idea was that an issue naming one of them has told the agent where to go, while one that does not has left it to search. Mean `resolution` score, out of 150:

| Issue names a file the fix touched | Issues | Mean resolution | Has a name resolving to exactly one thing |
|---|---:|---:|---:|
| yes | 60 | 67.9 | 98.3% |
| no | 210 | 56.0 | 95.2% |

Separation: **+11.9** points, which is nothing.

The last column explains why, and it is a flaw in the test rather than a finding about the score. Essentially every issue in the dataset -- 96% of them -- already contains at least one name that resolves to exactly one thing in its repository. Both groups are saturated, so there is no headroom for the label to discriminate.

The label is also asking a different question than it appears to. Naming the file a maintainer eventually chose to edit is not the same as being well grounded: an issue can point precisely at a symptom whose fix belongs somewhere else entirely. The test is kept and reported because it was proposed as evidence and produced none, which is worth saying out loud rather than dropping quietly.

## Caveats

- One index per repository, built at the base commit of that repository's first instance. Repositories evolve, but their vocabulary barely moves between nearby commits, and this turns three hundred downloads into a dozen. It is an approximation.
- SWE-bench issues are written by maintainers and users to be filed, not typed at a coding agent. They are far longer than a typical prompt.
- The cross-repository control shows the axis is repository-specific. It does not show that the resulting *number* predicts whether an agent will succeed; that would need agent runs, not scores.
- sphinx remains below chance. Its vocabulary appears in every other repository here, because they all build their documentation with sphinx: `toctree` sits in 244 of sphinx's own files and in 8 to 44 files of each of the others. Counting definition sites rather than mentions recovered part of this -- sphinx defines `toctree` fifteen times, the others define it never -- but a sphinx issue really is partly grounded in any project that uses sphinx, and that is not an error to be removed.
