//! `prepare` against a temporary, dependency-free workspace. Runs cargo offline only.

use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
};

use assert_cmd::Command as AssertCommand;
use predicates::str::contains;

const BASE: &str = "https://github.com/example/demo";
const MANIFEST: &str = "[package]\nname = \"ai-demo\"\n# Release version.\nversion = \"1.0.0\"   # synced by xtask\nedition = \"2024\"\n\n[dependencies]\n";

fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn changelog() -> String {
    format!(
        "# Changelog\n\n## [1.1.0] - 2026-10-14\n\n### Added\n\n- Add the `demo` flag ([`abc1234`]({BASE}/commit/abc1234def))\n\n## [1.0.0] - 2026-10-01\n\n_First release._\n\n[1.1.0]: {BASE}/releases/tag/ai-demo-v1.1.0\n[1.0.0]: {BASE}/releases/tag/ai-demo-v1.0.0\n"
    )
}

fn cargo(root: &Path, args: &[&str]) {
    let status = Command::new(env!("CARGO"))
        .args(args)
        .arg("--manifest-path")
        .arg(root.join("Cargo.toml"))
        .stdout(Stdio::null())
        .status()
        .expect("cargo runs");
    assert!(status.success(), "cargo {args:?} failed");
}

fn xtask(root: &Path, args: &[&str]) -> AssertCommand {
    let mut command = AssertCommand::cargo_bin("xtask").expect("binary should be built for integration tests");
    command.arg("--workspace-root").arg(root).args(args);
    command
}

#[test]
fn prepare_rewrites_only_the_package_version_and_its_lock_line() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    write(
        root,
        "Cargo.toml",
        &format!(
            "[workspace]\nmembers = [\"demo\"]\nresolver = \"3\"\n\n[workspace.package]\nrepository = \"{BASE}\"\n"
        ),
    );
    write(root, "demo/Cargo.toml", MANIFEST);
    write(root, "demo/src/main.rs", "fn main() {}\n");
    write(root, "demo/CHANGELOG.md", &changelog());
    cargo(root, &["generate-lockfile", "--offline"]);
    let lock_before = fs::read_to_string(root.join("Cargo.lock")).unwrap();

    xtask(root, &["check"]).assert().failure().stderr(contains("top release 1.1.0 does not match"));

    xtask(root, &["prepare", "ai-demo"]).env("CARGO", "false").assert().failure().stderr(contains("Restored"));
    assert_eq!(fs::read_to_string(root.join("demo/Cargo.toml")).unwrap(), MANIFEST);
    assert_eq!(fs::read_to_string(root.join("Cargo.lock")).unwrap(), lock_before);

    xtask(root, &["prepare", "ai-demo"])
        .assert()
        .success()
        .stdout(contains("ai-demo 1.0.0 -> 1.1.0"))
        .stdout(contains("OK ai-demo 1.1.0"))
        .stdout(contains("just release-publish ai-demo"));

    let manifest = fs::read_to_string(root.join("demo/Cargo.toml")).unwrap();
    assert_eq!(manifest, MANIFEST.replace("version = \"1.0.0\"   #", "version = \"1.1.0\"   #"));

    let lock_after = fs::read_to_string(root.join("Cargo.lock")).unwrap();
    let before: Vec<&str> = lock_before.lines().collect();
    let after: Vec<&str> = lock_after.lines().collect();
    assert_eq!(before.len(), after.len(), "lock line count changed:\n{lock_after}");
    let changed: Vec<(&str, &str)> =
        before.iter().zip(&after).filter(|(old, new)| old != new).map(|(o, n)| (*o, *n)).collect();
    assert_eq!(changed, [("version = \"1.0.0\"", "version = \"1.1.0\"")]);
    assert_eq!(lock_before.ends_with('\n'), lock_after.ends_with('\n'));
    cargo(root, &["metadata", "--locked", "--offline", "--format-version", "1"]);

    xtask(root, &["prepare", "ai-demo"])
        .assert()
        .failure()
        .stderr(contains("add the new release entry to the changelog first"));
    xtask(root, &["notes", "ai-demo", "--version", "1.0.0"]).assert().success().stdout("_First release._\n");
}
