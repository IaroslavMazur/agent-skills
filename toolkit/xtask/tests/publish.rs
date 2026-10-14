//! `publish` against a temporary repository with a bare `origin` and a stub `gh`. No network, no real `gh`.
#![cfg(unix)]

use std::{
    env,
    ffi::OsString,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

use assert_cmd::Command as AssertCommand;
use predicates::str::contains;
use tempfile::TempDir;

const TAG: &str = "ai-demo-v1.0.0";
const CHANGELOG: &str = "# Changelog\n\n## [1.0.0] - 2026-10-14\n\n_First release._\n\n[1.0.0]: https://github.com/example/demo/releases/tag/ai-demo-v1.0.0\n";
const GH_STUB: &str = r#"#!/bin/sh
printf '%s\n' "$*" >> "$GH_LOG"
if [ "$1 $2" = "release view" ]; then
  if [ -f "$GH_STATE" ]; then echo "title: existing"; exit 0; fi
  echo "release not found" >&2
  exit 1
fi
if [ "$1 $2" = "release create" ]; then
  while [ $# -gt 0 ]; do
    if [ "$1" = "--notes-file" ]; then cat "$2" > "$GH_NOTES"; fi
    shift
  done
  : > "$GH_STATE"
  exit 0
fi
echo "unexpected gh call" >&2
exit 2
"#;

/// Removes inherited Git environment, including `GIT_CONFIG_COUNT`/`GIT_CONFIG_KEY_*`/`GIT_CONFIG_VALUE_*`, and
/// ignores system and global configuration.
fn isolate(command: &mut Command) -> &mut Command {
    for key in inherited_git_vars() {
        command.env_remove(key);
    }
    command.env("GIT_CONFIG_NOSYSTEM", "1").env("GIT_CONFIG_GLOBAL", "/dev/null")
}

fn inherited_git_vars() -> Vec<OsString> {
    env::vars_os().map(|(key, _)| key).filter(|key| key.to_string_lossy().starts_with("GIT_")).collect()
}

fn git(dir: &Path, args: &[&str]) -> String {
    let output = isolate(Command::new("git").arg("-C").arg(dir).args(args)).output().expect("git runs");
    assert!(output.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn write(root: &Path, relative: &str, contents: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

struct Fixture {
    temp: TempDir,
    work: PathBuf,
    origin: PathBuf,
    initial: String,
    release: String,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let origin = temp.path().join("origin.git");
        let work = temp.path().join("work");
        git(temp.path(), &["init", "--quiet", "--bare", "--initial-branch=main", origin.to_str().unwrap()]);
        git(temp.path(), &["init", "--quiet", "--initial-branch=main", work.to_str().unwrap()]);
        for (key, value) in [
            ("user.name", "Test User"),
            ("user.email", "test@example.com"),
            ("commit.gpgsign", "false"),
            ("tag.gpgsign", "false"),
        ] {
            git(&work, &["config", key, value]);
        }
        git(&work, &["remote", "add", "origin", origin.to_str().unwrap()]);
        write(&work, "README.md", "demo\n");
        git(&work, &["add", "README.md"]);
        git(&work, &["commit", "--quiet", "-m", "Initial commit"]);
        git(&work, &["push", "--quiet", "--set-upstream", "origin", "main"]);
        let initial = git(&work, &["rev-parse", "HEAD"]);

        write(
            &work,
            "toolkit/Cargo.toml",
            "[workspace]\nmembers = [\"demo\", \"xtask\"]\nresolver = \"3\"\n\n[workspace.package]\nrepository = \"https://github.com/example/demo\"\n",
        );
        write(
            &work,
            "toolkit/demo/Cargo.toml",
            "[package]\nname = \"ai-demo\"\nversion = \"1.0.0\"\nedition = \"2024\"\n",
        );
        write(&work, "toolkit/demo/CHANGELOG.md", CHANGELOG);
        git(&work, &["add", "toolkit"]);
        git(&work, &["commit", "--quiet", "-m", "Release ai-demo 1.0.0"]);
        let release = git(&work, &["rev-parse", "HEAD"]);

        let bin = temp.path().join("bin");
        write(&bin, "gh", GH_STUB);
        fs::set_permissions(bin.join("gh"), fs::Permissions::from_mode(0o755)).unwrap();
        Self { temp, work, origin, initial, release }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.temp.path().join(name)
    }

    fn publish(&self) -> assert_cmd::assert::Assert {
        let mut command = AssertCommand::cargo_bin("xtask").expect("binary should be built for integration tests");
        for key in inherited_git_vars() {
            command.env_remove(key);
        }
        let search_path = env::join_paths(
            std::iter::once(self.path("bin")).chain(env::split_paths(&env::var_os("PATH").unwrap_or_default())),
        )
        .unwrap();
        command
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .current_dir(&self.work)
            .env("PATH", search_path)
            .env("GH_LOG", self.path("gh.log"))
            .env("GH_STATE", self.path("gh.state"))
            .env("GH_NOTES", self.path("gh.notes"))
            .arg("--workspace-root")
            .arg(self.work.join("toolkit"))
            .args(["publish", "ai-demo"])
            .assert()
    }

    fn gh_calls(&self) -> Vec<String> {
        fs::read_to_string(self.path("gh.log")).unwrap_or_default().lines().map(str::to_owned).collect()
    }

    fn remote_tag_commit(&self) -> String {
        git(&self.origin, &["rev-parse", &format!("refs/tags/{TAG}^{{commit}}")])
    }
}

#[test]
fn publish_requires_pushed_release_commit() {
    let fixture = Fixture::new();
    fixture.publish().failure().stderr(contains("push the release commit first"));
    assert!(fixture.gh_calls().is_empty());
    assert!(git(&fixture.work, &["tag", "--list"]).is_empty());
}

#[test]
fn publish_creates_tag_push_and_release_then_reruns_as_no_op() {
    let fixture = Fixture::new();
    git(&fixture.work, &["push", "--quiet", "origin", "main"]);

    fixture
        .publish()
        .success()
        .stdout(contains(format!("Created: local tag {TAG} at {}", fixture.release)))
        .stdout(contains(format!("Created: pushed tag {TAG} to origin")))
        .stdout(contains(format!("Created: GitHub release {TAG} on example/demo")));
    assert_eq!(git(&fixture.work, &["rev-parse", &format!("{TAG}^{{commit}}")]), fixture.release);
    assert_eq!(git(&fixture.work, &["cat-file", "-t", TAG]), "tag");
    assert_eq!(git(&fixture.work, &["tag", "--list", "--format=%(contents:subject)", TAG]), "ai-demo 1.0.0");
    assert_eq!(fixture.remote_tag_commit(), fixture.release);

    let calls = fixture.gh_calls();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert_eq!(calls[0], format!("release view {TAG} --repo example/demo"));
    let create = &calls[1];
    let prefix = format!("release create {TAG} --repo example/demo --verify-tag --title ai-demo 1.0.0 --notes-file ");
    assert!(create.starts_with(&prefix) && create.ends_with(" --latest=false"), "{create}");
    assert_eq!(fs::read_to_string(fixture.path("gh.notes")).unwrap(), "_First release._\n");

    fixture
        .publish()
        .success()
        .stdout(contains(format!("Skipped: local tag {TAG} already points to {}", fixture.release)))
        .stdout(contains(format!("Skipped: tag {TAG} already exists on origin")))
        .stdout(contains(format!("Skipped: GitHub release {TAG} already exists")));
    let calls = fixture.gh_calls();
    assert_eq!(calls.len(), 3, "{calls:?}");
    assert_eq!(calls[2], format!("release view {TAG} --repo example/demo"));
}

#[test]
fn publish_rejects_mismatched_local_tag() {
    let fixture = Fixture::new();
    git(&fixture.work, &["push", "--quiet", "origin", "main"]);
    git(&fixture.work, &["tag", "-a", TAG, &fixture.initial, "-m", "wrong"]);
    fixture.publish().failure().stderr(contains(format!(
        "local tag {TAG} points to {}, not release commit {}",
        fixture.initial, fixture.release
    )));
    assert!(fixture.gh_calls().is_empty());
}

#[test]
fn publish_rejects_mismatched_remote_tag() {
    let fixture = Fixture::new();
    git(&fixture.work, &["push", "--quiet", "origin", "main"]);
    // Push the wrong tag from another clone so that fetching does not copy it into the working repository.
    let other = fixture.path("other");
    git(fixture.temp.path(), &["clone", "--quiet", fixture.origin.to_str().unwrap(), other.to_str().unwrap()]);
    for (key, value) in [("user.name", "Other"), ("user.email", "other@example.com"), ("tag.gpgsign", "false")] {
        git(&other, &["config", key, value]);
    }
    git(&other, &["commit", "--quiet", "--allow-empty", "--no-gpg-sign", "-m", "Unrelated"]);
    let wrong = git(&other, &["rev-parse", "HEAD"]);
    git(&other, &["tag", "-a", TAG, "-m", "wrong"]);
    git(&other, &["push", "--quiet", "origin", &format!("refs/tags/{TAG}")]);

    fixture
        .publish()
        .failure()
        .stderr(contains(format!("tag {TAG} on origin points to {wrong}, not release commit {}", fixture.release)));
    assert_eq!(fixture.remote_tag_commit(), wrong);
    assert!(fixture.gh_calls().is_empty());
}
