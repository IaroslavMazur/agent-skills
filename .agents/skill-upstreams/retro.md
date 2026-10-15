# retro

- **Relationship:** confirmed adaptation of
  [mattpocock/skills](https://github.com/mattpocock/skills/tree/main/skills/engineering/retro),
  `skills/engineering/retro`.
- **Evidence:** the original request on 2026-10-05 named that source and required local history handling. The
  [catalog addition](https://github.com/PaulRBerg/agent-skills/commit/92df3f3ec2882bc72a0876c31cd4024bf9dc98a6) records
  the resulting skill.
- **Upstream revision:** unknown. The last reconciled upstream base is unknown. The catalog addition is not that base.

## Mapping and Local Behavior

Map upstream `SKILL.md` to `skills/retro/SKILL.md`. The local `agents/openai.yaml` has no established upstream mapping.

Preserve the `agents-introspection` dependency, task-selected history scope, recurrence evidence, report-only default,
and explicit apply authority. Preserve local invocation and report contracts. Do not replace history selection with a
fixed count of recent transcripts.

## Safe Update Boundary

Compare only the mapped entrypoint after recovering the base or approving a two-way reconciliation. Preserve local
metadata and history delegation. This record does not establish a baseline or authorize an update.
