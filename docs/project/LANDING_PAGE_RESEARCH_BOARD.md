# README Landing Page Research Board

Purpose: coordinate README landing-page research before changing the CX/XSHELF
root landing page.

The board should use two research agents, then decide the smallest README update
that improves orientation, trust, and first-run clarity without weakening the
project's deterministic/runtime-first positioning.

## Scope

Target surface:
- `README.md`

Current product frame:
- `XSHELF` is the primary public name.
- `CX` remains a compatibility name across the current command surface.
- The README must keep the independence/non-affiliation note.
- Rust runtime, structured JSON contracts, quarantine/replay, policy visibility,
  and repo-scoped state remain the core differentiators.

## Agent A: Developer Tool README Patterns

Role: research popular developer-tool README landings.

Reference set:
- `astral-sh/uv`: https://github.com/astral-sh/uv
- `vitejs/vite`: https://github.com/vitejs/vite
- `cli/cli`: https://github.com/cli/cli
- `BurntSushi/ripgrep`: https://github.com/BurntSushi/ripgrep
- `fastapi/fastapi`: https://github.com/fastapi/fastapi

Questions:
- How quickly does the README state the user-facing promise?
- Does the page show proof before a feature inventory?
- How many commands are needed before the reader sees first output?
- Which sections are above the fold: badges, visual proof, highlights,
  installation, quickstart, docs link, benchmarks, examples?
- How does the README separate beginner path from deep reference?

Expected output:
- five source notes with source URL, observed landing structure, and one
  transferable pattern
- three patterns to copy
- three patterns to avoid for CX/XSHELF
- one recommended top-of-README outline

## Agent B: AI Agent And Runtime README Patterns

Role: research popular AI-agent, LLM-runtime, and protocol README landings.

Reference set:
- `OpenHands/OpenHands`: https://github.com/OpenHands/OpenHands
- `langchain-ai/langchain`: https://github.com/langchain-ai/langchain
- `modelcontextprotocol/servers`: https://github.com/modelcontextprotocol/servers
- `anthropics/claude-code-action`: https://github.com/anthropics/claude-code-action

Questions:
- How does the README explain agent capability without overclaiming autonomy?
- Does it segment use cases by user intent, component, or deployment mode?
- Where does it introduce trust signals such as license, benchmarks, docs,
  community, security posture, or execution boundary?
- How does it describe provider/model flexibility?
- How does it prevent configuration detail from crowding the landing promise?

Expected output:
- four source notes with source URL, observed landing structure, and one
  transferable pattern
- three trust-building patterns for CX/XSHELF
- three phrasing risks to avoid
- one recommended agent/runtime positioning paragraph

## Board Intake

The board should accept only source-backed observations. Each agent should
produce notes in this shape:

```text
Source:
Landing order:
First-run path:
Trust signals:
What CX/XSHELF should borrow:
What CX/XSHELF should avoid:
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

After reading both agent reports, the board should produce:
- recommended README outline
- exact top-section copy draft
- commands to keep above the fold
- sections to move lower or split into docs
- risks and validation checklist
