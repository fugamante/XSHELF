# README Landing Page Research Board

Purpose: coordinate README landing-page research before changing the CX/XSHELF
root landing page.

The board should use three complementary reviewers, then decide the smallest
README update that improves orientation, trust, and first-run clarity without
weakening the project's deterministic/runtime-first positioning.

## Scope

Target surface:
- `README.md`

Current product frame:
- `XSHELF` is the primary public name.
- `CX` remains a compatibility name across the current command surface.
- The README must keep the independence/non-affiliation note.
- Rust runtime, structured JSON contracts, quarantine/replay, policy visibility,
  and repo-scoped state remain the core differentiators.

## Reviewer A: Industrial Design And Aesthetic Fit

Role: judge whether the landing page feels intentional, credible, and visually
scannable for a developer tool without adding ornamental noise.

Questions:
- Does the first screen establish hierarchy: name, promise, proof, then path?
- Are headings, bullets, code blocks, and notes visually balanced?
- Is the top section compact enough to scan without feeling underexplained?
- Do trust notes and compatibility notes feel deliberate rather than defensive?
- Which visual or structural elements should be removed, condensed, or moved?

Expected output:
- aesthetic diagnosis of the current top section
- recommended top-section structure and spacing rhythm
- copy or layout elements to keep, cut, or move lower
- risks where visual density could weaken trust or first-run clarity

## Reviewer B: Information Clarity And Message Utility

Role: condense constructs, clarify message utility, and ensure each term earns
its place in the first-run path.

Questions:
- What should a new reader know after 10 seconds, 60 seconds, and first run?
- Which terms need definition before they are useful?
- Which constructs can be collapsed into simpler user-facing outcomes?
- Does the page distinguish promise, mechanism, proof, and reference?
- Where does configuration detail crowd the landing promise?

Expected output:
- exact top-section copy proposal
- glossary or term-definition recommendations
- condensed feature grouping by user outcome
- risks where phrasing overclaims, obscures boundaries, or dilutes naming

## Reviewer C: Novice User Proxy

Role: represent a first-time user with basic technical chops: comfortable with
Git, shells, and JSON, but not already fluent in this project's runtime model.

Questions:
- Can the user explain what XSHELF is after one scan?
- Can the user identify the safest first command without reading deep docs?
- Which words, acronyms, or product names cause hesitation?
- Does the README show what success looks like after the first command?
- What is the next safe action if setup, policy, or backend choice is unclear?

Expected output:
- confusion log in reading order
- safe first-run path with expected output shape
- missing glossary terms or unclear aliases
- section-order recommendations from novice orientation

## Novice-Guided Refinement Loop

Reviewer C should produce findings before final synthesis. Reviewers A and B
must then update their recommendations in response:

- Reviewer A revises layout and visual hierarchy to remove novice hesitation.
- Reviewer B revises copy, term definitions, and condensation to answer novice
  questions without expanding the top section unnecessarily.
- The board synthesizes only after these revisions are complete.

## Board Intake

The board should accept only evidence-backed observations. Each reviewer should
produce notes in this shape:

```text
Reviewer:
Observation:
Evidence:
First-run impact:
Recommended change:
Risk if ignored:
```

## Board Decision Rubric

Score each proposed README change from 0 to 2:

- orientation: a new reader understands what XSHELF is within one scan
- first output: the README gets from clone to a meaningful command quickly
- proof: the page shows concrete behavior, not only capability claims
- trust: boundaries, compatibility, and non-affiliation are explicit
- maintainability: detailed reference stays below quickstart or in docs
- naming clarity: XSHELF leads while CX compatibility remains legible

Actuate changes only when a proposal scores at least 8/12 and does not regress
trust or naming clarity.

## Initial Synthesis

Current README gap:
- It is accurate, but it starts as a capability inventory. Popular README
  landings usually start with a short promise, proof or first output, install,
  and then a compact feature map.

Likely update direction:
- Keep `# XSHELF (formerly CX)`.
- Replace the opening with one strong promise and a concise "why use it" block.
- Move dense runtime internals below a shorter "Try it" path.
- Add a "First useful output" example using `doctor`, `health`, or a small
  `cxo` command with JSON inspection.
- Convert the long capability list into grouped outcomes:
  - deterministic command execution
  - structured diagnostics
  - task orchestration
  - backend policy and local model options
- Preserve existing compatibility and independence notes near the top.

## Board Output

After reading all reviewer reports and the novice-guided refinement pass, the
board should produce:
- recommended README outline
- exact top-section copy draft
- exact safe first-run path with expected output shape
- glossary or term-definition list for unclear constructs
- section-order recommendations
- commands to keep above the fold
- sections to move lower or split into docs
- risks and validation checklist
