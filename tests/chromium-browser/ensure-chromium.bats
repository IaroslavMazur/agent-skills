#!/usr/bin/env bats

# shellcheck disable=SC2030,SC2031,SC2154 # Bats provides its variables and runs tests in subshells.

setup_file() {
  bats_require_minimum_version 1.5.0
}

setup() {
  helper="$BATS_TEST_DIRNAME/../../skills/chromium-browser/scripts/ensure-chromium.sh"
  export HOME="$BATS_TEST_TMPDIR/home"
  export CURL_CALLS="$BATS_TEST_TMPDIR/curl-calls"
  export CURL_URLS="$BATS_TEST_TMPDIR/curl-urls"
  export OPEN_ARGS="$BATS_TEST_TMPDIR/open-args"
  unset PRB_AGENT_CHROMIUM_PORT CURL_UP_AFTER OPEN_EXIT
  stubs="$BATS_TEST_TMPDIR/bin"
  mkdir -p "$stubs"
  # The endpoint responds once curl has failed CURL_UP_AFTER times.
  cat >"$stubs/curl" <<'EOF'
#!/bin/bash
set -eu
calls=$(($(cat "$CURL_CALLS" 2>/dev/null || echo 0) + 1))
echo "$calls" >"$CURL_CALLS"
printf '%s\n' "${!#}" >>"$CURL_URLS"
[ "$calls" -gt "${CURL_UP_AFTER:-0}" ]
EOF
  cat >"$stubs/open" <<'EOF'
#!/bin/bash
printf '%s\n' "$@" >"$OPEN_ARGS"
exit "${OPEN_EXIT:-0}"
EOF
  printf '#!/bin/bash\n' >"$stubs/sleep"
  chmod +x "$stubs/curl" "$stubs/open" "$stubs/sleep"
  export PATH="$stubs:$PATH"
}

@test "does nothing when the endpoint already responds" {
  run /bin/bash "$helper"

  [ "$status" -eq 0 ]
  [ "$output" = 'Chromium is already listening on port 9222.' ]
  [ "$(cat "$CURL_URLS")" = 'http://127.0.0.1:9222/json/version' ]
  [ ! -e "$OPEN_ARGS" ]
}

@test "launches the shared profile and waits for the endpoint" {
  export CURL_UP_AFTER=3

  run /bin/bash "$helper"

  [ "$status" -eq 0 ]
  [ "$output" = 'Launched Chromium on port 9222.' ]
  expected_args="$(cat <<EOF
-na
Chromium
--args
--remote-debugging-port=9222
--user-data-dir=$HOME/Library/Application Support/prb-agent-chromium
--no-first-run
--no-default-browser-check
about:blank
EOF
)"
  [ "$(cat "$OPEN_ARGS")" = "$expected_args" ]
  [ "$(cat "$CURL_CALLS")" -eq 4 ]
}

@test "uses the custom port for the check and the launch" {
  export PRB_AGENT_CHROMIUM_PORT=9333
  export CURL_UP_AFTER=1

  run /bin/bash "$helper"

  [ "$status" -eq 0 ]
  [ "$(sort -u "$CURL_URLS")" = 'http://127.0.0.1:9333/json/version' ]
  grep -qx -- '--remote-debugging-port=9333' "$OPEN_ARGS"
}

@test "fails when Chromium cannot launch" {
  export CURL_UP_AFTER=1
  export OPEN_EXIT=1

  run -1 /bin/bash "$helper"

  [[ "$output" == *'Could not launch Chromium.app.'* ]]
  [ "$(cat "$CURL_CALLS")" -eq 1 ]
}

@test "fails when the endpoint never responds after launch" {
  export CURL_UP_AFTER=1000

  run -1 /bin/bash "$helper"

  [[ "$output" == *'Chromium did not open port 9222 within 20 seconds.'* ]]
  [ "$(cat "$CURL_CALLS")" -eq 41 ]
}
