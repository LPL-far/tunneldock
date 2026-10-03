# Research reasoning and readable evidence

## Source and adoption boundary

The user supplied two passages on 2026-10-03: an English note about controlled writing, diagrams, interactive HTML and bespoke explainer videos; and a Chinese article about six research-narrative paradigms. The earlier WeChat fetches returned verification pages. This protocol is based on the pasted text, not a claim that those URLs were successfully fetched or their authorship independently identified.

The article's submission counts, reviewer-time estimates, awards, 35-paper list, paper-specific performance numbers and causal explanations of award decisions are **source claims, not verified project facts**. Verify a conference award on its official site and a technical result in the actual paper before citing either as evidence. Do not turn these examples into accepted REFERENCES/RESULTS records automatically. No change to an existing method, benchmark, split or training run is authorized merely by this article.

External standard check: ASD-STE100 is a controlled technical English standard with writing rules and a dictionary. The official source is https://www.asd-ste100.org/about_STE.html ; AI-use caveats are at https://asd-ste100.org/STE_downloads.html (checked 2026-10-03). TunnelDock adopts clarity principles, **not verified ASD-STE100 compliance**. “80% of STE” is the supplied author's informal suggestion, not a conformity level. Chinese explanations are not STE-certified English.

## 1. Explain the result before expanding the format

The supplied note proposes a ladder: writing -> diagrams/images -> interactive HTML -> explainer video. Select by the reader's question, not by decoration or a requirement to produce every format.

- Default explanation: what changed, why it matters, what evidence supports it, what remains unknown, and the decision or next test.
- Use short complete sentences, one main point at a time, consistent terminology and concrete subjects. Define an unfamiliar term once. Preserve code identifiers, paths, numbers, units, negation, uncertainty and the exact conditions under which a claim holds.
- A diagram is useful for tensor/data flow, before/after contrasts or a causal hypothesis. Mark an assumed causal link as a hypothesis. A diagram cannot establish causality.
- An interactive HTML view is useful for examining curves, cases, alternatives and source-linked evidence. Keep original artifacts available and distinguish an excerpt from the complete result. Never present an unreviewed worker result as an accepted conclusion because it looks polished.
- Produce a video only when explicitly requested or clearly justified by a spatial/temporal mechanism that static evidence does not explain. Preserve its script, inputs and evidence references. Do not request, expose or use paid narration credentials as a default dependency. This release does not implement a narration/video service.
- Explanations are derived, disposable presentation artifacts. Rebuild them from versioned evidence. Do not delete the underlying evidence when discarding an explanation.

Codex owns implementation and deterministic verification. Gemini can inspect cases and produce explanatory visualizations. ChatGPT reads the necessary evidence and resolves interpretation; the human retains research-direction and acceptance decisions. A task completion, a delivered handoff, a readable explanation and an accepted scientific result are four different events.

## 2. Research reasoning, not cosmetic storytelling

The Chinese article's usable central claim is: narrative is a by-product of the research process, not makeup applied before submission. In TunnelDock this means an auditable chain:

**observed failure -> root-cause hypothesis -> competing explanation -> intervention -> decisive test -> evidence and boundary -> next decision**.

A score improvement alone does not identify its cause. A plausible story is not a verified mechanism. Do not describe an unsuccessful gate as passed, hide a negative dataset behind a macro average, or use a theoretical result outside its stated assumptions. Do not invent a theorem simply to match a narrative style.

### Six source paradigms and the evidence each asks for

| Article's paradigm | Question before implementation | Needed evidence / review boundary |
| --- | --- | --- |
| 根因手术刀 | What repeatable failure is observed, and why might it happen? | A controlled intervention distinguishing the proposed cause from at least one plausible alternative. Correlation alone is not a root-cause proof. |
| 反直觉重构 | Which default assumption is being challenged? | A clear statement of that assumption and a fair, matched replacement comparison. “Counterintuitive” is not itself novelty. |
| 理论照亮经验 | Under which assumptions does the explanation hold? | Definitions, assumptions, a valid derivation/proof and an explicit link to the empirical setting. Unproved claims remain hypotheses. |
| 新基准暴露失效 | What failure is missed by the current measurement? | A frozen protocol, leakage checks, examples, baselines and reliability checks. Do not tune the test set to make the preferred method win. |
| 社会价值叙事 | Who experiences the real problem and what would improve? | Task-relevant utility, operating constraints, limitations and possible harms. A motivation paragraph is not evidence of impact. |
| 极简统一美学 | Can one representation or interface replace unnecessary mechanisms? | An explicit interface, fair multi-task tests, cost and boundary cases. Fewer modules do not prove a better method. |

These categories are lenses for analysis, not scores, mandatory architecture types, a template that every paper must satisfy, or a way to choose a narrative after cherry-picking favorable results. Select a main line only when the project's existing evidence supports it.

## 3. Compact handoff and review questions

Keep existing task/run IDs, response budgets and HANDOFF delivery. Do not launch unrelated experiments merely to fill a template.

For an implementation task, retain the current Outcome / Summary / Files changed / Verification / Evidence / Cleanup / Remaining risks / Requested review fields. Explain the decisive change in plain language; keep exact commands and evidence paths.

For a research decision, include a brief **Research reasoning** section when applicable, within the existing response budget:

1. Observation: measured behavior, source path, run and protocol.
2. Hypothesis: proposed mechanism, explicitly provisional until tested.
3. Alternatives: plausible confound or competing explanation.
4. Decisive test: smallest discriminating test; freeze its criterion before looking at new target outcomes.
5. Falsifier: an outcome that would weaken or refute the hypothesis.
6. Evidence and limits: supporting AND negative cases, seed/data/code versions, uncertainty, compute and missing evidence.
7. Next decision: keep, revise, reject or gather evidence. A recommendation is not the human's recorded decision.

Use NOT_TESTED / UNKNOWN when evidence is missing. A source's statement, a worker's interpretation and a reviewed project conclusion must remain explicitly distinguishable. Do not infer novelty, causality or acceptance from a title, an award badge, SOTA, a passing software test or a local completion footer.

The UI's structured handoff reader only segments original text; it does not summarize, fact-check, accept a task, infer a causal graph, or set web_reviewed. An empty section means “not present on this page”, not “no risk”. Review all required pages before giving the final conclusion.

## 4. Memory, compression and cleanup

Store accepted research facts in the existing domain files; do not create another canonical memory tree. PROJECT_STATE and SESSION_HANDOFF remain current continuation points. MODEL_DESIGN records assumptions and interfaces, EXPERIMENTS records interventions and negative results, RESULTS records accepted outcomes, and REFERENCES stores verified source identity/provenance.

When updating or compressing memory, preserve the distinction between observation/hypothesis/interpretation/decision, unresolved contradictions, evaluation rules, active blockers and meaningful negative results. The automatic 12 KiB working set is incomplete; omitted constraints still exist in its original hashed sources. Inspect them before accepting or rejecting a method. No keyword heuristic can prove that all necessary facts survived extraction.

Only supersede old conclusions with an explicit reason and replacement evidence. Never delete results, source material or a scientific failure merely to make the presentation shorter. This protocol changes generated guidance and reading presentation, not the scientific content of the three active projects.
