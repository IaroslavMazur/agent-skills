# codebase-design

- **Relationship:** confirmed fork and adaptation of
  [mattpocock/skills](https://github.com/mattpocock/skills/tree/main/skills/engineering/codebase-design),
  `skills/engineering/codebase-design`.
- **Evidence:** the
  [catalog addition](https://github.com/PaulRBerg/agent-skills/commit/b5ef809e34a398c7528794b4ec8d9e2f35e26b2f)
  explicitly records a fork and adaptation with catalog metadata and references. The local entrypoint retains
  attribution.
- **Upstream revision:** unknown. The last reconciled upstream base is unknown.

## Mapping and Local Behavior

Map upstream paths to `skills/codebase-design/` as follows:

| Upstream             | Local                           |
| -------------------- | ------------------------------- |
| `SKILL.md`           | `SKILL.md`                      |
| `DEEPENING.md`       | `references/DEEPENING.md`       |
| `DESIGN-IT-TWICE.md` | `references/DESIGN-IT-TWICE.md` |

`references/AGENT-LEGIBILITY.md` is a local addition. `agents/openai.yaml` is locally maintained catalog metadata.

Preserve local deep-module vocabulary, agent-legibility guidance, ownership and repository-context conventions,
reference navigation, and output contract. Verify the current upstream layout before extending the mapping.

## Safe Update Boundary

Recover the base or approve a two-way reconciliation before merging mapped files. Treat local-only references and
locally maintained metadata as local behavior. Do not infer an upstream revision from the catalog import commit.
