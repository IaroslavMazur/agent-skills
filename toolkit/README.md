# Toolkit

The CLI and local-app subtree of [agent-skills](../README.md): agent-oriented command-line tools and local applications
for shared-working-tree coordination, notifications, commits, skill catalogs, and task handoffs.

- [ai-commit](commit/): prepare and commit immutable Git snapshots safely in shared working trees.
- [ai-coord](coord/): coordinate parallel Codex and Claude Code agents.
- [ai-handoff](handoff/): create and archive agent task handoffs.
- [ai-notify](notify/): deliver desktop notifications for Claude Code and Codex CLI.
- [ai-skillet](skillet/): inspect and maintain agent-skill catalogs.
- [Coordination dashboard](apps/coord-dashboard/): local live view of ai-coord state and a read-only task-handoff
  viewer, running at [https://coord.localhost](https://coord.localhost) (handoffs at
  [https://coord.localhost/handoffs](https://coord.localhost/handoffs)).

For contribution guidance and validation, see [AGENTS.md](AGENTS.md).

## Platform support

| CLI          | macOS | Linux |
| ------------ | ----- | ----- |
| `ai-commit`  | Yes   | Yes   |
| `ai-coord`   | Yes   | Yes   |
| `ai-handoff` | Yes   | Yes   |
| `ai-notify`  | Yes   | No    |
| `ai-skillet` | Yes   | Yes   |

Windows is not supported. The CLIs do not configure background services on Linux. The existing ai-coord service and
dashboard setup uses user-specific macOS launchd configuration. On Linux, `ai-coord` requires readable `/proc` process
information.

## Installation

Install Rust nightly, Git 2.41 or newer, and a C compiler toolchain. The C toolchain builds bundled SQLite for
`ai-coord`.

On macOS or Linux:

```sh
cargo +nightly install --locked --git https://github.com/PaulRBerg/agent-skills ai-commit ai-coord ai-handoff ai-skillet
```

On macOS, also install `ai-notify`:

```sh
cargo +nightly install --locked --git https://github.com/PaulRBerg/agent-skills ai-notify
```

Cargo installs into `~/.cargo/bin` by default. Add that directory to `PATH`.

From a clone, install [just](https://github.com/casey/just) and run this command from the repository root:

```sh
just toolkit::install-cli
```

This recipe installs into `~/.local/bin`. Add that directory to `PATH`. On Linux, it installs the four supported CLIs.
On macOS, it also installs `ai-notify` and restarts an existing loaded `local.ai-coord-api` launchd service.

### Linux handoff clipboard

For clipboard copying, install [wl-clipboard](https://github.com/bugaevc/wl-clipboard) on Wayland or
[xclip](https://github.com/astrand/xclip) on X11. These packages provide the commands that `ai-handoff` uses to copy and
verify a handoff prompt. In a headless environment, use `ai-handoff create --no-clipboard`.

Cross-repository handoffs require an existing `$HOME/Desktop` directory, including on Linux.

## Versions

Each CLI follows [SemVer](https://semver.org/) independently and records its releases in its `CHANGELOG.md`, in
[Common Changelog](https://common-changelog.org/) format: [ai-commit](commit/CHANGELOG.md),
[ai-coord](coord/CHANGELOG.md), [ai-handoff](handoff/CHANGELOG.md), [ai-notify](notify/CHANGELOG.md), and
[ai-skillet](skillet/CHANGELOG.md).

Tags use the format `<crate>-v<version>`, for example `ai-coord-v1.0.0`. Each tag has a
[GitHub release](https://github.com/PaulRBerg/agent-skills/releases). To install a release:

```sh
cargo +nightly install --locked --git https://github.com/PaulRBerg/agent-skills --tag ai-coord-v1.0.0 ai-coord
```
