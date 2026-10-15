# autoresearch

- **Relationship:** port from `pi-autoresearch`. The exact imported repository and source path are unresolved.
- **Evidence:** the
  [initial addition](https://github.com/PaulRBerg/agent-skills/commit/56457169e16955842a52371fad8019c32129e66b)
  explicitly identifies the port, including JSONL experiment records, MAD confidence, and a dashboard.
  [davebcn87/pi-autoresearch](https://github.com/davebcn87/pi-autoresearch) exists and has matching project behavior,
  but it is only a candidate source. No recovered evidence confirms that repository as the imported source.
- **Upstream revision:** unknown. The imported source path and last reconciled base are unknown.

## Mapping and Local Behavior

No upstream-to-local file mapping is verified. Current local files include `skills/autoresearch/SKILL.md`,
`references/loop-rules.md`, `references/worker-loop.md`, and the bundled `scripts/` helpers.

The [bounded-batch redesign](https://github.com/PaulRBerg/agent-skills/commit/9d4c7b467eef49c4edb8e92595add2e74aecd78f)
introduced local orchestration and task contracts. Preserve bounded experiments, delegated execution, experiment
evidence, and the current helper contracts.

## Safe Update Boundary

Confirm the imported repository, path, and file mapping before comparing external changes. Recover the base or obtain
approval for a two-way reconciliation. Do not schedule the candidate repository or treat its current files as a verified
base.
