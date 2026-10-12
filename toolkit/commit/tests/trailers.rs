mod common;

use std::fs;

use common::{Harness, exit_code, stderr, stdout, write_executable};

const TRAILER_SHIM: &str =
    "#!/bin/sh\nif [ \"$1\" = trailer ]; then printf 'Agent-Session: codex:test-session\\n'; exit 0; fi\nexit 1\n";

fn resolving_shim(log: &str) -> String {
    format!(
        "#!/bin/sh\nif [ \"$1\" = trailer ]; then printf 'Agent-Session: codex:test-session\\n'; exit 0; fi\n\
         if [ \"$1\" = finding ]; then printf '%s\\n' \"$*\" >> '{log}'; printf 'RESOLVED\\t%s\\tfixed\\n' \"$3\"; exit \
         0; fi\nexit 1\n"
    )
}

fn prepared(harness: &Harness) -> String {
    harness.write("intended.txt", "base\n");
    harness.commit_all("base");
    harness.write("intended.txt", "changed\n");
    harness.prepare(&["intended.txt"]).0
}

#[test]
fn commit_appends_closes_paragraph_and_trailer_block_and_resolves_findings() {
    let harness = Harness::new("trailers");
    let log = harness.root.join("resolve.log");
    write_executable(&harness.shim.join("ai-coord"), &resolving_shim(&log.display().to_string()));
    let transaction = prepared(&harness);

    let output = harness.success([
        "commit",
        &transaction,
        "-m",
        "feat: add thing",
        "-m",
        "- body line",
        "--close",
        "12",
        "--close",
        "12",
        "--close",
        "7",
        "--finding",
        "abc123",
        "--finding",
        "abc123",
    ]);
    let receipt = stdout(&output);
    assert!(receipt.contains("COMMITTED "), "{receipt}");
    assert!(receipt.contains("FINDING_RESOLVED abc123\n"), "{receipt}");
    assert_eq!(receipt.matches("FINDING_RESOLVED").count(), 1);

    let message = harness.git(["log", "-1", "--format=%B"]);
    assert_eq!(
        message,
        "feat: add thing\n\n- body line\n\nCloses #12\nCloses #7\n\nFinding-ID: abc123\nAgent-Session: codex:test-session"
    );
    assert_eq!(harness.git(["log", "-1", "--format=%(trailers:key=Finding-ID,valueonly)"]), "abc123");

    let full_oid = harness.git(["rev-parse", "HEAD"]);
    assert_eq!(fs::read_to_string(&log).unwrap(), format!("finding resolve abc123 --as fixed --commit {full_oid}\n"));
}

#[test]
fn trailers_merge_into_an_existing_trailer_paragraph_without_duplicates() {
    let harness = Harness::new("merge");
    write_executable(&harness.shim.join("ai-coord"), TRAILER_SHIM);
    let transaction = prepared(&harness);

    harness.success([
        "commit",
        &transaction,
        "-m",
        "feat: add thing",
        "-m",
        "Co-authored-by: Pair <pair@example.com>\nAgent-Session: codex:test-session",
        "--close",
        "3",
    ]);
    assert_eq!(
        harness.git(["log", "-1", "--format=%B"]),
        "feat: add thing\n\nCloses #3\n\nCo-authored-by: Pair <pair@example.com>\nAgent-Session: codex:test-session"
    );
}

#[test]
fn a_subject_only_message_gets_a_separate_trailer_paragraph() {
    let harness = Harness::new("subject");
    write_executable(&harness.shim.join("ai-coord"), TRAILER_SHIM);
    let transaction = prepared(&harness);

    harness.success(["commit", &transaction, "-m", "Note: single paragraph"]);
    assert_eq!(
        harness.git(["log", "-1", "--format=%B"]),
        "Note: single paragraph\n\nAgent-Session: codex:test-session"
    );
}

#[test]
fn messages_stay_untouched_without_coordination_or_flags() {
    let harness = Harness::new("plain");
    let transaction = prepared(&harness);
    harness.success(["commit", &transaction, "-m", "feat: add thing", "-m", "body"]);
    assert_eq!(harness.git(["log", "-1", "--format=%B"]), "feat: add thing\n\nbody");
}

#[test]
fn failed_finding_resolution_keeps_the_commit_and_is_retryable() {
    let harness = Harness::new("unresolved");
    let transaction = prepared(&harness);

    write_executable(
        &harness.shim.join("ai-coord"),
        "#!/bin/sh\nif [ \"$1\" = finding ]; then echo 'error: unknown finding' >&2; exit 1; fi\nexit 1\n",
    );
    let failed = harness.command(["commit", &transaction, "-m", "feat: add thing", "--finding", "abc123"]);
    assert_eq!(exit_code(&failed), 3, "{}", stderr(&failed));
    let receipt = stdout(&failed);
    assert!(receipt.contains("COMMITTED "), "{receipt}");
    assert!(receipt.contains("FINDING_UNRESOLVED abc123\n"), "{receipt}");
    assert!(stderr(&failed).contains("abc123: error: unknown finding"), "{}", stderr(&failed));
    assert!(stderr(&failed).contains("ai-coord finding resolve <id> --as fixed --commit"), "{}", stderr(&failed));
    let head = harness.git(["rev-parse", "HEAD"]);
    assert_eq!(harness.git(["log", "-1", "--format=%B"]), "feat: add thing\n\nFinding-ID: abc123");

    let log = harness.root.join("resolve.log");
    write_executable(&harness.shim.join("ai-coord"), &resolving_shim(&log.display().to_string()));
    let retried = harness.success(["commit", &transaction, "-m", "feat: add thing", "--finding", "abc123"]);
    let receipt = stdout(&retried);
    assert!(receipt.contains("COMMITTED "), "{receipt}");
    assert!(receipt.contains("FINDING_RESOLVED abc123\n"), "{receipt}");
    assert_eq!(harness.git(["rev-parse", "HEAD"]), head, "retry must not create a second commit");
    assert_eq!(fs::read_to_string(&log).unwrap(), format!("finding resolve abc123 --as fixed --commit {head}\n"));
}

#[test]
fn close_and_finding_values_are_validated() {
    let harness = Harness::new("flags");
    let transaction = prepared(&harness);
    for arguments in [
        ["--close", "0"],
        ["--close", "012"],
        ["--close", "#12"],
        ["--close", ""],
        ["--finding", ""],
        ["--finding", "a b"],
        ["--finding", "a/b"],
    ] {
        let output = harness.command(["commit", &transaction, "-m", "feat: add thing", arguments[0], arguments[1]]);
        assert_eq!(exit_code(&output), 2, "{arguments:?}: {}", stderr(&output));
    }
    assert_eq!(harness.git(["log", "-1", "--format=%s"]), "base");
}

#[test]
fn prepare_discloses_paths_outside_the_active_claim_only_when_a_claim_exists() {
    let harness = Harness::new("unclaimed");
    let log = harness.root.join("coord.log");
    harness.write("intended.txt", "base\n");
    harness.write("docs/guide.md", "base\n");
    harness.write("other.txt", "base\n");
    harness.commit_all("base");
    harness.write("intended.txt", "changed\n");
    harness.write("docs/guide.md", "changed\n");
    harness.write("other.txt", "changed\n");

    write_executable(
        &harness.shim.join("ai-coord"),
        &format!(
            "#!/bin/sh\necho \"$1\" >> '{}'\nif [ \"$1\" = scope ]; then printf 'docs\\trecursive\\nintended.txt\\texact\\n'; \
             exit 0; fi\nexit 1\n",
            log.display()
        ),
    );
    let (_, porcelain) = harness.prepare(&["intended.txt", "docs/guide.md", "other.txt"]);
    assert!(porcelain.contains("UNCLAIMED\tother.txt\n"), "{porcelain}");
    assert_eq!(porcelain.matches("UNCLAIMED\t").count(), 1, "{porcelain}");
    let ordinary = stdout(&harness.success(["prepare", "--", "other.txt"]));
    assert!(
        ordinary.contains("## unclaimed paths (outside this session's active ai-coord claim)\nother.txt\n"),
        "{ordinary}"
    );

    harness.git(["add", "other.txt"]);
    fs::write(&log, "").unwrap();
    let staged = stdout(&harness.success(["prepare", "--porcelain", "--staged"]));
    assert!(!staged.contains("UNCLAIMED"), "{staged}");
    assert!(!fs::read_to_string(&log).unwrap().contains("scope"), "--staged must not consult the claim");

    write_executable(&harness.shim.join("ai-coord"), "#!/bin/sh\nif [ \"$1\" = scope ]; then exit 0; fi\nexit 1\n");
    let (_, no_claim) = harness.prepare(&["other.txt"]);
    assert!(!no_claim.contains("UNCLAIMED"), "{no_claim}");
}
