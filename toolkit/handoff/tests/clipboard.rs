mod common;

use std::{fs, process::Output};

#[cfg(target_os = "linux")]
use common::stdout;
use common::{Harness, stderr};

fn create(harness: &Harness, environment: &[(&str, &str)], skip: bool) -> (std::path::PathBuf, Output) {
    let repo = harness.repo("repo", true);
    let draft = harness.root.join("draft.md");
    fs::write(&draft, "# Clipboard\n").unwrap();
    let mut arguments = vec![
        "create",
        "--repo",
        repo.to_str().unwrap(),
        "--category",
        "audit",
        "--task",
        "clipboard test",
        "--draft",
        draft.to_str().unwrap(),
        "CLIPBOARD.md",
    ];
    if skip {
        arguments.push("--no-clipboard");
    }
    (repo.clone(), harness.command_with_env(arguments, environment))
}

#[test]
fn clipboard_writer_failure_rolls_back_publication() {
    let harness = Harness::new("copy-failure");
    common::write_executable(&harness.clipboard_writer(), "#!/bin/sh\nexit 1\n");
    let (repo, output) = create(&harness, &[], false);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("clipboard copy failed"));
    assert!(!repo.join(".ai").exists());
    assert!(output.stdout.is_empty());
}

#[test]
fn clipboard_reader_failure_rolls_back_publication() {
    let harness = Harness::new("read-failure");
    common::write_executable(&harness.clipboard_reader(), "#!/bin/sh\nexit 1\n");
    let (repo, output) = create(&harness, &[], false);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("clipboard readback failed"));
    assert!(!repo.join(".ai").exists());
    assert!(output.stdout.is_empty());
}

#[cfg(target_os = "linux")]
#[test]
fn linux_uses_x11_when_wayland_tools_are_missing() {
    let harness = Harness::new("x11-fallback");
    fs::remove_file(harness.root.join("shim/wl-copy")).unwrap();
    let (repo, output) = create(&harness, &[("DISPLAY", ":99")], false);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(repo.join(".ai/task-handoffs/CLIPBOARD.md").is_file());
    let output = stdout(&output);
    let command = output.lines().find_map(|line| line.strip_prefix("command\t")).unwrap();
    assert_eq!(fs::read(&harness.clipboard).unwrap(), command.as_bytes());
}

#[cfg(target_os = "linux")]
#[test]
fn linux_ignores_nonexecutable_wayland_tools() {
    use std::os::unix::fs::PermissionsExt;

    let harness = Harness::new("nonexecutable-tools");
    fs::set_permissions(harness.root.join("shim/wl-copy"), fs::Permissions::from_mode(0o644)).unwrap();
    let (repo, output) = create(&harness, &[], false);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("--no-clipboard"));
    assert!(!repo.join(".ai").exists());
}

#[cfg(target_os = "linux")]
#[test]
fn linux_headless_requires_no_clipboard_even_with_tools_on_path() {
    for skip in [false, true] {
        let harness = Harness::new("headless");
        let (repo, output) = create(&harness, &[("WAYLAND_DISPLAY", ""), ("DISPLAY", "")], skip);
        assert_eq!(output.status.success(), skip, "{}", stderr(&output));
        assert_eq!(repo.join(".ai/task-handoffs/CLIPBOARD.md").exists(), skip);
        assert!(!harness.clipboard.exists());
        if !skip {
            assert!(stderr(&output).contains("WAYLAND_DISPLAY"));
            assert!(stderr(&output).contains("wl-clipboard"));
            assert!(stderr(&output).contains("xclip"));
            assert!(stderr(&output).contains("--no-clipboard"));
        }
    }
}
