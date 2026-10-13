#!/usr/bin/env bash
set -euo pipefail

toolkit_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
just_bin="$(command -v just)"
test_dir="$(mktemp -d)"
trap 'rm -rf "$test_dir"' EXIT
mkdir "$test_dir/bin"

cat >"$test_dir/bin/cargo" <<'MOCK'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$MOCK_CARGO_LOG"
MOCK
cat >"$test_dir/bin/uname" <<'MOCK'
#!/usr/bin/env bash
printf '%s\n' "$MOCK_OS"
MOCK
cat >"$test_dir/bin/launchctl" <<'MOCK'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$MOCK_LAUNCHCTL_LOG"
if [[ "$1" == print ]]; then
    exit "$MOCK_SERVICE_STATUS"
fi
MOCK
cat >"$test_dir/bin/id" <<'MOCK'
#!/usr/bin/env bash
printf '1234\n'
MOCK
chmod +x "$test_dir/bin/"*

export PATH="$test_dir/bin:$PATH"
export MOCK_CARGO_LOG="$test_dir/cargo.log"
export MOCK_LAUNCHCTL_LOG="$test_dir/launchctl.log"
export MOCK_OS MOCK_SERVICE_STATUS

run_install() {
    MOCK_OS="$1"
    MOCK_SERVICE_STATUS="$2"
    : >"$MOCK_CARGO_LOG"
    : >"$MOCK_LAUNCHCTL_LOG"
    "$just_bin" --quiet --justfile "$toolkit_dir/justfile" install-cli
}

assert_packages() {
    local package
    : >"$test_dir/expected-cargo.log"
    for package in "$@"; do
        printf 'install --quiet --path %s --locked --force --root %s/.local\n' "$package" "$HOME" \
            >>"$test_dir/expected-cargo.log"
    done
    diff -u "$test_dir/expected-cargo.log" "$MOCK_CARGO_LOG"
}

run_install Linux 0
assert_packages commit coord handoff skillet
[[ ! -s "$MOCK_LAUNCHCTL_LOG" ]]

run_install Darwin 0
assert_packages commit coord handoff notify skillet
printf '%s\n' 'print gui/1234/local.ai-coord-api' 'kickstart -k gui/1234/local.ai-coord-api' \
    >"$test_dir/expected-launchctl.log"
diff -u "$test_dir/expected-launchctl.log" "$MOCK_LAUNCHCTL_LOG"

run_install Darwin 1
assert_packages commit coord handoff notify skillet
printf '%s\n' 'print gui/1234/local.ai-coord-api' >"$test_dir/expected-launchctl.log"
diff -u "$test_dir/expected-launchctl.log" "$MOCK_LAUNCHCTL_LOG"

if run_install FreeBSD 0 >"$test_dir/unsupported.log" 2>&1; then
    printf 'Expected installation to reject FreeBSD\n' >&2
    exit 1
fi
grep -F 'Unsupported operating system: FreeBSD (expected macOS or Linux)' "$test_dir/unsupported.log"
[[ ! -s "$MOCK_CARGO_LOG" && ! -s "$MOCK_LAUNCHCTL_LOG" ]]

printf 'install-cli tests passed\n'
