# autoresearch

- **Status:** shelved. The committed skill was archived verbatim from source commit
  [669d42602b3b0d134d3175f82851e94a80ec263f](https://github.com/PaulRBerg/agent-skills/commit/669d42602b3b0d134d3175f82851e94a80ec263f).
- **Relationship:** port from `pi-autoresearch`. The exact imported repository and source path are unresolved.
- **Evidence:** the
  [initial addition](https://github.com/PaulRBerg/agent-skills/commit/56457169e16955842a52371fad8019c32129e66b)
  explicitly identifies the port, including JSONL experiment records, MAD confidence, and a dashboard.
  [davebcn87/pi-autoresearch](https://github.com/davebcn87/pi-autoresearch) exists and has matching project behavior,
  but it is only a candidate source. No recovered evidence confirms that repository as the imported source.
- **Upstream revision:** unknown. The imported source path and last reconciled base are unknown.

## Mapping and Local Behavior

No upstream-to-local file mapping is verified. Archived files include `skills/autoresearch/SKILL.md`,
`references/loop-rules.md`, `references/worker-loop.md`, and the bundled `scripts/` helpers.

The [bounded-batch redesign](https://github.com/PaulRBerg/agent-skills/commit/9d4c7b467eef49c4edb8e92595add2e74aecd78f)
introduced local orchestration and task contracts. Preserve bounded experiments, delegated execution, experiment
evidence, and the current helper contracts.

## Safe Update Boundary

Do not restore this skill or add it to scheduled rebase targets. Before an authorized restoration, confirm the imported
repository, path, and file mapping. Recover the base or obtain approval for a two-way reconciliation. Do not treat the
candidate repository as a verified source or base.
