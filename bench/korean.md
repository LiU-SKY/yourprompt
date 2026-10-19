# Korean: text that says nothing must not outscore text that asks for something

`bench/korean.txt` is 32 Korean prompts in five tiers, from a held-down key to
a fully specified request. The claim under test is ordering -- a higher tier
outranks a lower one -- plus two hard ceilings: tier 0 (not language) stays
under 150, tier 1 (language, but no request) under 350. The regression test
`the_korean_fixture_holds_its_ordering` scores it without an index, so the
result does not drift as this repository grows, and runs in CI. The numbers
below are scored the same way unless they say otherwise.

## The honest starting point

The report that started this was a user typing 아 over eight lines and
watching it score 346, in a status line whose index had taken in their whole
home directory; without an index it was 409.2. Measured without an index on
the commit before the fix (ab75e0d) and on v0.1.1:

| | before | v0.1.1 |
|---|---:|---:|
| cross-tier ordering (395 pairs) | 74.7% | **97.5%** |
| tier-0 ceiling breaches | 5 | **0** |
| tier 1 vs 2 boundary | 39.6% | 91.7% |
| worst tier-0 score | 476.5 ("테스트" ×10) | 61.5 |

Against this repository's own index, v0.1.1 orders 98.7% of the pairs (390 of
395).

The 39.6% is the number worth staring at: a complaint with no object (409.2)
outscored every tier-2 request, "그거 좀 고쳐줘" (239.8) included. Making a
request exposed you to penalties; making none exposed you to nothing. The
scorer was punishing people for asking.

## What was wrong, in order of embarrassment

All four defects are the same defect -- **absence of evidence read as evidence
of quality** -- reaching clarity and deixis through gaps the English fixtures
never probed:

1. **Repetition was invisible.** "테스트" typed ten times is ten legible
   content tokens, zero ambiguity smells, clarity 250/250. Absence credit is
   now scaled by lexical variety (root TTR against
   `DIVERSITY_FULL_RTTR`) -- the information in a query lives in its distinct
   terms, which is why pre-retrieval QPP's AvICTF is defined over them.
2. **A non-request could not lose points it never put at stake.** Femmer et
   al.'s requirements smells presuppose a requirement; a greeting contains no
   vague words *about the work* because it contains no work. With no
   objective, no I/O shape, no acceptance criterion and no scope boundary,
   absence credit is cut to `NO_REQUEST_FACTOR` (0.4).
3. **A held key passed as written Korean.** "아아아아아아" is syllable blocks,
   so the jamo-mash check waved it through. One short unit repeated three or
   more times is now mash; standalone interjections (아, 음, 어...) are
   neutral, like punctuation -- neither language nor mash, and never content.
4. **Digits counted as language.** A screen of numbers was 87% "legible" and
   scored 357.2. Numbers are now neutral in the legibility share; a number
   inside a sentence still costs nothing.

One cue-list bug rode along: bare "테스트" was an *acceptance* cue, so the
repeated-word garbage was collecting completion-criteria points. An acceptance
criterion is a condition, not a topic; every Korean acceptance term now
carries the condition with it ("테스트가 통과", "완료 기준", ...).

Two coverage gaps in the Korean lexicons surfaced the same day: verbs whose
stem ends in a noun ("최적화해줘", "리팩터링해줘") never matched because the
matcher's boundary rule rejects a following 해-syllable, so the 해-forms are
now listed explicitly; and hedges ("모르겠", "어떻게 해야", "어렵다") joined
the uncertainty smells, which is what separates thinking-aloud from asking.

## What did not move

The fix was required not to buy Korean at English's expense. Measured when it
landed:

| benchmark | before | after |
|---|---:|---:|
| HumanEvalComm, all pairs | 52.5% | 53.0% |
| HumanEvalComm, lexically reachable | 65.4% | 65.9% |
| SWE own-vs-foreign repository | 87.4% (+42.7) | 87.4% (+42.7) |

## What is still allowed to be imperfect

Ten inversions remain, all at the floor. Four are "저거 빨리 좀 어떻게 해줘
급해" (57.8) scoring under tier-1 small talk: it is nominally a request, but
one that tells an agent nothing at all, and the scorer is not wrong to say so.
The other six are the two loudest kinds of tier-0 noise, "테스트" typed ten
times (60.4) and filler syllables (61.5), a few points above the greeting
(41.0), thinking aloud (56.9) and that same urgent non-request. Against this
repository's index five remain: the same four, and the filler syllables (61.3)
over the greeting (56.8).

And "src/auth/login.rs 의 토큰 만료 처리 로직을 수정해줘" (tier 3) scores
328.8 without an index but 293.7 *against this repository*, which has no such
file -- naming things that do not exist where you are standing earns nothing,
and that is the grounding axis doing its job, not failing at it.
