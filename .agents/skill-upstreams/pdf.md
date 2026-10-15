# pdf

- **Relationship:** replacement of an external skill, not a rebased fork.
- **Replaced source:** [anthropics/skills](https://github.com/anthropics/skills/tree/main/skills/pdf), `skills/pdf`.
- **Evidence:** the initial request on 2026-08-08 proposed a fork. The approved plan instead required a clean-room
  replacement without copied upstream prose, scripts, fixtures, or assets. The
  [implementation](https://github.com/PaulRBerg/agent-skills/commit/aae0bb19f899b00a4257e33849171cf89b70c57e) records
  the replacement.
- **Upstream revision:** no reconciled upstream revision or mechanical merge base applies to this replacement.

## Mapping and Local Behavior

There is no upstream merge mapping. Local files include `skills/pdf/SKILL.md`, `references/forms.md`,
`references/recipes.md`, `scripts/profile.py`, `scripts/form.py`, and `agents/openai.yaml`.

Preserve the local macOS extraction, OCR, manipulation, and form workflows and their helper contracts.

## Safe Update Boundary

Do not import external code by treating this replacement as an existing fork. Evaluate external changes as separate
proposals within the user's authorized scope. This record does not establish an upstream baseline.
