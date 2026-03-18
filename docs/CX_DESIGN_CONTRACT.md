# CX Design Contract

Status: Draft (v1)  
Last Updated: 2026-03-18

## Purpose
Establish a stable design discipline for CX surfaces (CLI, docs, UI patterns in companion repos) so behavior stays coherent as scope grows.

## Principles
1. Function first: every element must improve orientation, action, or feedback.
2. Cohesion over novelty: preserve interaction grammar across features.
3. Human-scale clarity: summary-first, bounded modules, explicit labels.
4. Progressive disclosure: calm defaults, deep detail on demand.
5. Readability discipline: concise names and operator-centered language.

## Naming Readability Rule
- Default maximum: 3 segments (2 underscores) for new file stems, functions, and tests.
- Legacy over-limit names are allowed only through explicit allowlists.
- New work must not expand allowlists without documented migration rationale.

## UX Invariants
- Top-level state/mode/risk must be visible immediately.
- Related controls must keep consistent size and spacing rhythm.
- Mutating actions require explicit confirmation and clear consequence messaging.
- Blocked actions must explain why they are blocked.

## Review Gate
1. Can an operator orient in one scan?
2. Are next actions obvious and safely scoped?
3. Is detail hidden unless needed?
4. Is language concise and unambiguous?
5. Did naming stay within readability limits?

