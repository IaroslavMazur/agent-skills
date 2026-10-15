#!/usr/bin/env bats

# shellcheck disable=SC2154 # Bats provides BATS_TEST_DIRNAME.
bats_require_minimum_version 1.5.0

# Check shell syntax with macOS's system Bash. This does not prove full Bash 3.2 compatibility.
@test "production shell helpers parse under /bin/bash" {
  [[ -x /bin/bash ]] || skip "/bin/bash is missing"
  local repo_root script
  repo_root="$(cd "$BATS_TEST_DIRNAME/.." && pwd)"
  for script in "$repo_root"/skills/*/scripts/*.sh "$repo_root"/mcp/*.sh "$repo_root"/scripts/*.sh; do
    [[ -f "$script" ]] || continue
    /bin/bash -n "$script" || {
      echo "syntax error under /bin/bash: $script" >&2
      return 1
    }
  done
}
