---
description: Explain the score of your last prompt, axis by axis
allowed-tools: Bash(yp explain:*), PowerShell(yp explain:*)
disable-model-invocation: true
---

Here is the breakdown of the last prompt you scored:

!`yp explain --no-color`

If it says the last prompt was not graded, tell the user why in one sentence
and stop.

Otherwise, read the breakdown above and tell the user, in two or three
sentences:

- which single component cost them the most points, and
- one concrete rewrite of their prompt that would raise it.

Be specific about *their* prompt. Do not lecture about prompt engineering in
general, and do not restate the numbers -- they can already see those.
