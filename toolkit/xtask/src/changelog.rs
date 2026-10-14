//! Validation and release-note extraction for Common Changelog files.

use std::{ops::Range, sync::LazyLock};

use regex::Regex;
use semver::Version;

/// Change groups in their required order.
const GROUPS: [&str; 4] = ["Changed", "Added", "Removed", "Fixed"];
const BREAKING: &str = "**Breaking:** ";

static HEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^## \[([^\]]+)\] - (\d{4})-(\d{2})-(\d{2})$").expect("valid heading regex"));
static DEFINITION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\[[^\]]+\]: \S").expect("valid definition regex"));

/// One validation error, attached to a 1-based line number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub line: usize,
    pub message: String,
}

impl Diagnostic {
    fn new(line: usize, message: impl Into<String>) -> Self {
        Self { line, message: message.into() }
    }
}

/// Formats diagnostics as `<path>:<line>: <message>` lines.
pub fn format_diagnostics(path: &str, diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics.iter().map(|d| format!("{path}:{}: {}", d.line, d.message)).collect()
}

/// A release entry in a valid changelog.
#[derive(Debug)]
pub struct Release {
    pub version: Version,
    /// 1-based line number of the release heading.
    pub line: usize,
    /// Line indices of the body, between the heading and the next release heading or link definitions.
    body: Range<usize>,
}

/// A changelog that passed every validation rule.
#[derive(Debug)]
pub struct Changelog {
    lines: Vec<String>,
    releases: Vec<Release>,
}

/// The kind of SemVer step between two adjacent releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Major,
    Minor,
    Patch,
}

/// The three valid next versions after `from`: major, minor, and patch.
pub fn next_versions(from: &Version) -> [Version; 3] {
    [
        Version::new(from.major + 1, 0, 0),
        Version::new(from.major, from.minor + 1, 0),
        Version::new(from.major, from.minor, from.patch + 1),
    ]
}

/// Classifies `to` as a valid next version after `from`, or `None` when it is not one.
pub fn step(from: &Version, to: &Version) -> Option<Step> {
    let [major, minor, patch] = next_versions(from);
    if *to == major {
        Some(Step::Major)
    } else if *to == minor {
        Some(Step::Minor)
    } else if *to == patch {
        Some(Step::Patch)
    } else {
        None
    }
}

/// Describes the valid next versions after `from` for error messages.
pub fn expected_next(from: &Version) -> String {
    let [major, minor, patch] = next_versions(from);
    format!("{major}, {minor}, or {patch}")
}

impl Changelog {
    /// Validates `text` against the changelog rules. `base` is the repository URL and `crate_name` the package
    /// name used in release tag links.
    pub fn parse(text: &str, base: &str, crate_name: &str) -> Result<Self, Vec<Diagnostic>> {
        Parser::new(text, base).run(crate_name)
    }

    #[cfg(test)]
    pub fn releases(&self) -> &[Release] {
        &self.releases
    }

    /// The newest release. Valid changelogs always have one.
    pub fn top(&self) -> &Release {
        &self.releases[0]
    }

    /// The release body for `version`, with surrounding blank lines trimmed.
    pub fn notes(&self, version: &Version) -> Option<String> {
        let release = self.releases.iter().find(|release| release.version == *version)?;
        let mut body = &self.lines[release.body.clone()];
        while body.first().is_some_and(|line| line.is_empty()) {
            body = &body[1..];
        }
        while body.last().is_some_and(|line| line.is_empty()) {
            body = &body[..body.len() - 1];
        }
        Some(format!("{}\n", body.join("\n")))
    }
}

/// A release heading as parsed, before cross-release checks.
struct Draft {
    version: Option<Version>,
    line: usize,
    body: Range<usize>,
    breaking: bool,
    added: bool,
}

struct Parser<'a> {
    lines: Vec<&'a str>,
    base: &'a str,
    commit_link: Regex,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str, base: &'a str) -> Self {
        let trimmed = text.trim_end_matches('\n');
        let lines: Vec<&str> = trimmed.split('\n').collect();
        let mut diagnostics = Vec::new();
        if !text.ends_with('\n') {
            diagnostics.push(Diagnostic::new(lines.len(), "file must end with a newline"));
        } else if text.len() - trimmed.len() > 1 {
            diagnostics
                .push(Diagnostic::new(lines.len() + 1, "only one final newline may follow the link definitions"));
        }
        let commit_link =
            Regex::new(&format!(r"\[`([0-9a-f]{{7,40}})`\]\({}/commit/([0-9a-f]+)\)", regex::escape(base)))
                .expect("valid commit link regex");
        Self { lines, base, commit_link, diagnostics }
    }

    fn error(&mut self, line: usize, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic::new(line, message));
    }

    fn run(mut self, crate_name: &str) -> Result<Changelog, Vec<Diagnostic>> {
        if self.lines[0] != "# Changelog" {
            self.error(1, "line 1 must be `# Changelog`");
        }
        if self.lines.get(1) != Some(&"") {
            self.error(2, "line 2 must be blank");
        }

        let mut definitions_start = self.lines.len();
        while definitions_start > 2 && DEFINITION.is_match(self.lines[definitions_start - 1]) {
            definitions_start -= 1;
        }

        let headings: Vec<usize> =
            (2..definitions_start).filter(|&index| self.lines[index].starts_with("## ")).collect();
        match headings.first() {
            None => self.error(3, "the changelog must contain at least one release heading `## [X.Y.Z] - YYYY-MM-DD`"),
            Some(&first) if first > 2 => self.error(3, "unexpected content before the first release heading"),
            Some(_) => {}
        }

        let mut drafts = Vec::new();
        for (position, &heading) in headings.iter().enumerate() {
            let end = headings.get(position + 1).copied().unwrap_or(definitions_start);
            let version = self.heading(heading);
            let body = heading + 1..end;
            let (breaking, added) = self.body(heading, body.clone());
            drafts.push(Draft { version, line: heading + 1, body, breaking, added });
        }

        self.steps(&drafts);
        self.definitions(&drafts, definitions_start, crate_name);

        if !self.diagnostics.is_empty() {
            self.diagnostics.sort_by_key(|d| d.line);
            return Err(self.diagnostics);
        }
        let releases = drafts
            .into_iter()
            .map(|draft| Release {
                version: draft.version.expect("valid changelogs have parsed versions"),
                line: draft.line,
                body: draft.body,
            })
            .collect();
        Ok(Changelog { lines: self.lines.iter().map(|line| (*line).to_owned()).collect(), releases })
    }

    /// Validates a release heading and returns its version when the version is usable for ordering checks.
    fn heading(&mut self, index: usize) -> Option<Version> {
        let line = index + 1;
        let Some(captures) = HEADING.captures(self.lines[index]) else {
            self.error(line, "release heading must match `## [X.Y.Z] - YYYY-MM-DD`");
            return None;
        };
        let (year, month, day) =
            (captures[2].parse().unwrap(), captures[3].parse().unwrap(), captures[4].parse().unwrap());
        if !valid_date(year, month, day) {
            self.error(line, format!("invalid calendar date {}-{}-{}", &captures[2], &captures[3], &captures[4]));
        }
        match Version::parse(&captures[1]) {
            Ok(version) if version.pre.is_empty() && version.build.is_empty() => Some(version),
            Ok(version) => {
                self.error(
                    line,
                    format!("version {version} must be a stable X.Y.Z release without prerelease or build metadata"),
                );
                None
            }
            Err(error) => {
                self.error(line, format!("invalid version `{}`: {error}", &captures[1]));
                None
            }
        }
    }

    /// Validates a release body and reports whether it has breaking items and an `### Added` group.
    fn body(&mut self, heading: usize, body: Range<usize>) -> (bool, bool) {
        let heading_line = heading + 1;
        if body.is_empty() || !self.lines[body.start].is_empty() {
            self.error(heading_line, "a blank line must follow the release heading");
        }
        if !body.is_empty() && !self.lines[body.end - 1].is_empty() {
            self.error(
                body.end,
                "a blank line must separate this line from the next release heading or link definitions",
            );
        }

        let mut blocks: Vec<Vec<usize>> = Vec::new();
        let mut previous_blank = true;
        for index in body.clone() {
            if self.lines[index].is_empty() {
                if previous_blank && index > body.start {
                    self.error(index + 1, "blocks must be separated by exactly one blank line");
                }
                previous_blank = true;
            } else {
                if previous_blank {
                    blocks.push(Vec::new());
                }
                blocks.last_mut().expect("block was started").push(index);
                previous_blank = false;
            }
        }

        let mut notice = false;
        let mut groups = 0;
        let mut last_group: Option<usize> = None;
        let mut breaking = false;
        let mut added = false;
        let mut position = 0;
        while position < blocks.len() {
            let block = blocks[position].clone();
            let first = self.lines[block[0]];
            let line = block[0] + 1;
            position += 1;
            if first.starts_with('#') {
                groups += 1;
                match first.strip_prefix("### ") {
                    Some(name) => match GROUPS.iter().position(|group| *group == name) {
                        Some(order) => {
                            match last_group {
                                Some(last) if last == order => self.error(line, format!("duplicate group `{name}`")),
                                Some(last) if last > order => self.error(
                                    line,
                                    format!("group `{name}` must come before `{}`; use the order Changed, Added, Removed, Fixed", GROUPS[last]),
                                ),
                                _ => last_group = Some(order),
                            }
                            added |= name == "Added";
                        }
                        None => {
                            self.error(line, format!("unknown group `{name}`; use Changed, Added, Removed, or Fixed"))
                        }
                    },
                    None => self.error(line, "unexpected heading; a release body allows only `### <group>` headings"),
                }
                let items = if block.len() > 1 {
                    self.error(block[1] + 1, "a blank line must follow the group heading");
                    block[1..].to_vec()
                } else if blocks.get(position).is_some_and(|next| self.lines[next[0]].starts_with("- ")) {
                    position += 1;
                    blocks[position - 1].clone()
                } else {
                    self.error(line, "a group heading must be followed by a blank line and a list of `- ` items");
                    Vec::new()
                };
                breaking |= self.items(&items);
            } else if is_notice(first) {
                if block.len() > 1 {
                    self.error(block[1] + 1, "the notice must be a single line");
                }
                if notice {
                    self.error(line, "a release body allows at most one notice");
                } else if position > 1 {
                    self.error(line, "the notice must be the first block of the release body");
                }
                notice = true;
            } else if first.starts_with("- ") {
                self.error(line, "list items must follow a `### <group>` heading");
            } else {
                self.error(line, "unexpected content; a release body allows only an italic notice and change groups");
            }
        }
        if !notice && groups == 0 {
            self.error(heading_line, "a release body must contain a notice or at least one group");
        }
        (breaking, added)
    }

    /// Validates the list lines of one group and reports whether any item is breaking.
    fn items(&mut self, indices: &[usize]) -> bool {
        let mut breaking = false;
        let mut seen_regular = false;
        for &index in indices {
            let line = index + 1;
            let Some(text) = self.lines[index].strip_prefix("- ") else {
                self.error(line, "each item must be a single `- ` line; continuation lines are not allowed");
                continue;
            };
            let rest = match text.strip_prefix(BREAKING) {
                Some(rest) => {
                    breaking = true;
                    if seen_regular {
                        self.error(line, "breaking items must come before non-breaking items in their group");
                    }
                    rest
                }
                None => {
                    seen_regular = true;
                    text
                }
            };
            if !rest.chars().next().is_some_and(char::is_uppercase) {
                self.error(line, "item text must start with an uppercase letter");
            }
            if !text.ends_with(')') {
                self.error(line, "item must end with `)`");
            }
            let linked = self.commit_link.captures_iter(text).any(|captures| captures[2].starts_with(&captures[1]));
            if !linked {
                self.error(line, format!("item must contain a commit link [`<hash>`]({}/commit/<hash>)", self.base));
            }
        }
        breaking
    }

    /// Checks descending order and SemVer steps between adjacent releases.
    fn steps(&mut self, drafts: &[Draft]) {
        for pair in drafts.windows(2) {
            let (newer, older) = (&pair[0], &pair[1]);
            let (Some(new), Some(old)) = (&newer.version, &older.version) else { continue };
            if new <= old {
                self.error(
                    older.line,
                    format!("version {old} must be lower than {new} above it; releases must strictly descend"),
                );
                continue;
            }
            let Some(kind) = step(old, new) else {
                self.error(
                    newer.line,
                    format!("{new} is not a valid next version after {old}; expected {}", expected_next(old)),
                );
                continue;
            };
            if newer.breaking && !(kind == Step::Major || (old.major == 0 && kind == Step::Minor)) {
                let required = if old.major == 0 { "a minor or major" } else { "a major" };
                self.error(newer.line, format!("breaking changes require {required} version step from {old}"));
            }
            if newer.added && kind == Step::Patch {
                self.error(
                    newer.line,
                    format!("an `### Added` group requires a minor or major version step from {old}"),
                );
            }
        }
    }

    /// Checks that the file ends with one link definition per release, in release order.
    fn definitions(&mut self, drafts: &[Draft], start: usize, crate_name: &str) {
        let definitions = start..self.lines.len();
        for (position, draft) in drafts.iter().enumerate() {
            let expected = draft
                .version
                .as_ref()
                .map(|version| format!("[{version}]: {}/releases/tag/{crate_name}-v{version}", self.base));
            match (definitions.clone().nth(position), expected) {
                (None, Some(expected)) => self.error(draft.line, format!("missing link definition `{expected}`")),
                (None, None) => self.error(draft.line, "missing link definition for this release"),
                (Some(index), Some(expected)) if self.lines[index] != expected => {
                    self.error(index + 1, format!("link definition must be `{expected}`"));
                }
                _ => {}
            }
        }
        for index in definitions.skip(drafts.len()) {
            self.error(index + 1, "extra link definition without a matching release");
        }
    }
}

fn is_notice(line: &str) -> bool {
    line.len() >= 3 && line.starts_with('_') && line.ends_with('_')
}

fn valid_date(year: u32, month: u32, day: u32) -> bool {
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}

#[cfg(test)]
#[path = "changelog_tests.rs"]
mod tests;
