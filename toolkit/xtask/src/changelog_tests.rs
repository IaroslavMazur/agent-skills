use semver::Version;

use super::{Changelog, valid_date};

const BASE: &str = "https://github.com/PaulRBerg/agent-skills";
const CRATE: &str = "ai-coord";

const BASELINE: &str = "# Changelog

## [1.0.0] - 2026-10-14

_First release._

[1.0.0]: https://github.com/PaulRBerg/agent-skills/releases/tag/ai-coord-v1.0.0
";

const VALID: &str = "# Changelog

## [2.0.0] - 2026-10-14

_Breaking release._

### Changed

- **Breaking:** Rename the `--foo` flag to `--bar` ([`abc1234`]({B}/commit/abc1234def))
- Improve status output ([#12](https://github.com/PaulRBerg/agent-skills/pull/12)) ([`abc1234`]({B}/commit/abc1234))

### Added

- Add the `bar` command ([`deadbee`]({B}/commit/deadbeef00))

### Fixed

- Fix a crash on empty input ([`0123456`]({B}/commit/0123456789abcdef))

## [1.1.0] - 2024-02-29

### Added

- Add a thing ([`1234567`]({B}/commit/1234567))

## [1.0.1] - 2026-01-01

### Fixed

- Fix a thing ([`1234567`]({B}/commit/1234567aaa))

## [1.0.0] - 2025-12-31

_First release._

[2.0.0]: {B}/releases/tag/ai-coord-v2.0.0
[1.1.0]: {B}/releases/tag/ai-coord-v1.1.0
[1.0.1]: {B}/releases/tag/ai-coord-v1.0.1
[1.0.0]: {B}/releases/tag/ai-coord-v1.0.0
";

fn valid() -> String {
    VALID.replace("{B}", BASE)
}

fn item(text: &str) -> String {
    format!("- {text} ([`abc1234`]({BASE}/commit/abc1234ff))")
}

/// Builds a changelog from `(version, body)` pairs, newest first.
fn build(releases: &[(&str, &str)]) -> String {
    let mut text = String::from("# Changelog\n\n");
    for (version, body) in releases {
        text.push_str(&format!("## [{version}] - 2026-10-14\n\n{body}\n\n"));
    }
    for (version, _) in releases {
        text.push_str(&format!("[{version}]: {BASE}/releases/tag/{CRATE}-v{version}\n"));
    }
    text
}

/// 1-based line number of the first line containing `needle`.
fn line(text: &str, needle: &str) -> usize {
    text.lines().position(|line| line.contains(needle)).expect("needle is present") + 1
}

#[track_caller]
fn assert_error(text: &str, line: usize, fragment: &str) {
    let diagnostics = Changelog::parse(text, BASE, CRATE).expect_err("changelog should be invalid");
    assert!(
        diagnostics.iter().any(|d| d.line == line && d.message.contains(fragment)),
        "expected line {line} to report `{fragment}`, got {diagnostics:#?}"
    );
}

#[track_caller]
fn assert_valid(text: &str) -> Changelog {
    Changelog::parse(text, BASE, CRATE).unwrap_or_else(|diagnostics| panic!("expected valid, got {diagnostics:#?}"))
}

#[test]
fn baseline_notice_only_file_is_valid() {
    let changelog = assert_valid(BASELINE);
    assert_eq!(changelog.top().version, Version::new(1, 0, 0));
    assert_eq!(changelog.top().line, 3);
}

#[test]
fn multi_release_file_with_groups_is_valid() {
    let changelog = assert_valid(&valid());
    let versions: Vec<String> = changelog.releases().iter().map(|r| r.version.to_string()).collect();
    assert_eq!(versions, ["2.0.0", "1.1.0", "1.0.1", "1.0.0"]);
}

#[test]
fn zero_major_breaking_change_needs_only_minor_step() {
    let breaking = format!("### Changed\n\n{}", item("**Breaking:** Drop the old flag"));
    assert_valid(&build(&[("0.2.0", &breaking), ("0.1.0", "_First._")]));
}

#[test]
fn notes_extract_trimmed_release_body() {
    let changelog = assert_valid(&valid());
    let notes = changelog.notes(&Version::new(1, 1, 0)).unwrap();
    assert_eq!(notes, format!("### Added\n\n- Add a thing ([`1234567`]({BASE}/commit/1234567))\n"));
    assert_eq!(changelog.notes(&Version::new(1, 0, 0)).unwrap(), "_First release._\n");
    let top = changelog.notes(&Version::new(2, 0, 0)).unwrap();
    assert!(top.starts_with("_Breaking release._\n\n### Changed\n"));
    assert!(top.ends_with("0123456789abcdef))\n"));
    assert_eq!(changelog.notes(&Version::new(9, 9, 9)), None);
}

#[test]
fn rejects_wrong_title_and_missing_blank_second_line() {
    assert_error(&BASELINE.replace("# Changelog\n", "# Change log\n"), 1, "`# Changelog`");
    assert_error(&BASELINE.replace("# Changelog\n\n", "# Changelog\nIntro\n\n"), 2, "line 2 must be blank");
}

#[test]
fn rejects_bad_heading_format() {
    let text = valid().replace("## [1.1.0] - 2024-02-29", "## 1.1.0 - 2024-02-29");
    assert_error(&text, line(&text, "## 1.1.0"), "must match `## [X.Y.Z] - YYYY-MM-DD`");
    let text = BASELINE.replace("## [1.0.0] - 2026-10-14", "## [1.0.0] 2026-10-14");
    assert_error(&text, 3, "must match");
}

#[test]
fn rejects_invalid_calendar_dates() {
    let text = valid().replace("2026-01-01", "2026-02-29");
    assert_error(&text, line(&text, "2026-02-29"), "invalid calendar date 2026-02-29");
    assert_error(&BASELINE.replace("2026-10-14", "2026-13-01"), 3, "invalid calendar date");
    assert!(valid_date(2000, 2, 29) && valid_date(2024, 2, 29));
    assert!(!valid_date(1900, 2, 29) && !valid_date(2026, 4, 31) && !valid_date(2026, 1, 0));
}

#[test]
fn rejects_prerelease_and_build_versions() {
    assert_error(&BASELINE.replace("## [1.0.0]", "## [1.0.0-rc.1]"), 3, "stable X.Y.Z");
    assert_error(&BASELINE.replace("## [1.0.0]", "## [1.0.0+build.5]"), 3, "stable X.Y.Z");
    assert_error(&BASELINE.replace("## [1.0.0]", "## [1.0]"), 3, "invalid version");
}

#[test]
fn rejects_non_descending_versions() {
    let text = build(&[("1.0.0", "_A._"), ("1.1.0", "_B._")]);
    assert_error(&text, line(&text, "## [1.1.0]"), "must strictly descend");
    let text = build(&[("1.0.0", "_A._"), ("1.0.0", "_B._")]);
    assert_error(&text, 7, "must strictly descend");
}

#[test]
fn rejects_skipped_versions() {
    let text = build(&[("1.2.0", "_A._"), ("1.0.0", "_B._")]);
    assert_error(&text, 3, "not a valid next version after 1.0.0; expected 2.0.0, 1.1.0, or 1.0.1");
    let text = build(&[("1.1.1", "_A._"), ("1.0.0", "_B._")]);
    assert_error(&text, 3, "not a valid next version");
}

#[test]
fn rejects_breaking_change_without_major_step() {
    let breaking = format!("### Changed\n\n{}", item("**Breaking:** Drop the old flag"));
    let text = build(&[("1.1.0", &breaking), ("1.0.0", "_First._")]);
    assert_error(&text, 3, "breaking changes require a major version step from 1.0.0");
    let text = build(&[("0.1.1", &breaking), ("0.1.0", "_First._")]);
    assert_error(&text, 3, "breaking changes require a minor or major version step from 0.1.0");
}

#[test]
fn rejects_added_group_without_minor_step() {
    let added = format!("### Added\n\n{}", item("Add a flag"));
    let text = build(&[("1.0.1", &added), ("1.0.0", "_First._")]);
    assert_error(&text, 3, "`### Added` group requires a minor or major version step");
}

#[test]
fn rejects_out_of_order_groups() {
    let body = format!("### Fixed\n\n{}\n\n### Changed\n\n{}", item("Fix a"), item("Change b"));
    let text = build(&[("1.0.0", &body)]);
    assert_error(&text, line(&text, "### Changed"), "must come before `Fixed`");
}

#[test]
fn rejects_duplicate_groups() {
    let body = format!("### Fixed\n\n{}\n\n### Fixed\n\n{}", item("Fix a"), item("Fix b"));
    let text = build(&[("1.0.0", &body)]);
    assert_error(&text, 9, "duplicate group `Fixed`");
}

#[test]
fn rejects_unknown_groups_and_other_headings() {
    let text = build(&[("1.0.0", &format!("### Security\n\n{}", item("Patch a hole")))]);
    assert_error(&text, 5, "unknown group `Security`");
    let text = build(&[("1.0.0", &format!("#### Fixed\n\n{}", item("Fix a")))]);
    assert_error(&text, 5, "unexpected heading");
}

#[test]
fn rejects_items_without_commit_link() {
    let text = build(&[("1.0.0", "### Fixed\n\n- Fix a crash (#12)")]);
    assert_error(&text, 7, "must contain a commit link");
    let mismatched = format!("### Fixed\n\n- Fix a crash ([`abc1234`]({BASE}/commit/fff1234))");
    assert_error(&build(&[("1.0.0", &mismatched)]), 7, "must contain a commit link");
    let other_repo = "### Fixed\n\n- Fix a crash ([`abc1234`](https://github.com/x/y/commit/abc1234))";
    assert_error(&build(&[("1.0.0", other_repo)]), 7, "must contain a commit link");
}

#[test]
fn rejects_item_style_violations() {
    assert_error(&build(&[("1.0.0", &format!("### Fixed\n\n{}", item("fix a crash")))]), 7, "uppercase letter");
    let body = format!("### Fixed\n\n- Fix a crash ([`abc1234`]({BASE}/commit/abc1234)).");
    assert_error(&build(&[("1.0.0", &body)]), 7, "must end with `)`");
}

#[test]
fn rejects_breaking_items_after_regular_items() {
    let body = format!("### Changed\n\n{}\n{}", item("Tweak a"), item("**Breaking:** Remove b"));
    let text = build(&[("2.0.0", &body), ("1.0.0", "_First._")]);
    assert_error(&text, 8, "breaking items must come before non-breaking items");
}

#[test]
fn rejects_stray_content() {
    let text = build(&[("1.0.0", "_First._\n\nSome paragraph.")]);
    assert_error(&text, 7, "unexpected content");
    let text = build(&[("1.0.0", &format!("### Fixed\n\n{}\n\n```sh\nls\n```", item("Fix a")))]);
    assert_error(&text, 9, "unexpected content");
    let text = build(&[("1.0.0", &format!("### Fixed\n\n{}\n\n1. Numbered", item("Fix a")))]);
    assert_error(&text, 9, "unexpected content");
    let text = build(&[("1.0.0", &item("Orphan item"))]);
    assert_error(&text, 5, "must follow a `### <group>` heading");
}

#[test]
fn rejects_multi_line_items() {
    let body = format!("### Fixed\n\n{}\n  continued here", item("Fix a crash"));
    assert_error(&build(&[("1.0.0", &body)]), 8, "continuation lines are not allowed");
}

#[test]
fn rejects_bad_blank_line_layout() {
    let text = build(&[("1.0.0", &format!("### Fixed\n\n\n{}", item("Fix a")))]);
    assert_error(&text, 7, "exactly one blank line");
    let text = build(&[("1.0.0", &format!("### Fixed\n{}", item("Fix a")))]);
    assert_error(&text, 6, "a blank line must follow the group heading");
    assert_error(&BASELINE.replace("\n\n_First", "\n_First"), 3, "a blank line must follow the release heading");
    assert_error(&BASELINE.replace("._\n\n[1.0.0]", "._\n[1.0.0]"), 5, "a blank line must separate");
    assert_error(&build(&[("1.0.0", "")]), 3, "must contain a notice or at least one group");
}

#[test]
fn rejects_missing_extra_and_mismatched_definitions() {
    let text = valid();
    let missing = text.replace(&format!("[1.0.0]: {BASE}/releases/tag/ai-coord-v1.0.0\n"), "");
    assert_error(&missing, line(&missing, "## [1.0.0]"), "missing link definition");
    let extra = format!("{text}[0.9.0]: {BASE}/releases/tag/ai-coord-v0.9.0\n");
    assert_error(&extra, line(&extra, "[0.9.0]:"), "extra link definition");
    let mismatched = text.replace("tag/ai-coord-v1.1.0", "tag/ai-notify-v1.1.0");
    assert_error(&mismatched, line(&mismatched, "ai-notify-v1.1.0"), "link definition must be");
    let reordered = text.replace("[1.1.0]: ", "[tmp]: ").replace("[1.0.1]: ", "[1.1.0]: ");
    assert_error(&reordered, line(&reordered, "[tmp]:"), "link definition must be");
}

#[test]
fn rejects_content_after_definitions() {
    assert_error(&format!("{BASELINE}\n"), 8, "only one final newline");
    assert_error(BASELINE.trim_end(), 7, "must end with a newline");
}

#[test]
fn rejects_misplaced_and_duplicate_notices() {
    let body = format!("### Fixed\n\n{}\n\n_Late notice._", item("Fix a"));
    let text = build(&[("1.0.0", &body)]);
    assert_error(&text, 9, "notice must be the first block");
    let text = build(&[("1.0.0", "_One._\n\n_Two._")]);
    assert_error(&text, 7, "at most one notice");
}
