//! Workspace discovery and byte-preserving package version rewrites.

use std::{
    fs,
    ops::Range,
    path::{Path, PathBuf},
};

use semver::Version;
use toml_edit::{Document, Item, Table};

/// A CLI package in the workspace.
#[derive(Debug)]
pub struct Cli {
    pub name: String,
    /// Member directory relative to the workspace root.
    pub member: String,
    pub version: Version,
}

/// The toolkit workspace and its releasable CLIs.
#[derive(Debug)]
pub struct Workspace {
    pub root: PathBuf,
    /// Repository URL without a trailing slash, used in changelog links.
    pub base: String,
    /// GitHub `owner/repo` slug.
    pub slug: String,
    pub clis: Vec<Cli>,
}

impl Workspace {
    /// Reads the workspace manifest and every member manifest except `xtask`.
    pub fn load(root: &Path) -> Result<Self, String> {
        let path = root.join("Cargo.toml");
        let display = path.display().to_string();
        let text = read(&path)?;
        let document = parse(&display, &text)?;
        let workspace =
            table(document.as_table(), "workspace").ok_or_else(|| format!("{display}:1: missing [workspace] table"))?;
        let members = workspace
            .get("members")
            .and_then(Item::as_array)
            .ok_or_else(|| format!("{display}:1: missing [workspace] members array"))?;
        let repository = table(workspace, "package")
            .and_then(|package| package.get("repository"))
            .and_then(Item::as_str)
            .ok_or_else(|| format!("{display}:1: missing [workspace.package] repository"))?;
        let base = repository.trim_end_matches('/').to_owned();
        let slug = github_slug(&base).ok_or_else(|| {
            format!("{display}:1: [workspace.package] repository `{repository}` is not a https://github.com/<owner>/<repo> URL")
        })?;

        let mut clis = Vec::new();
        for member in members.iter() {
            let member = member.as_str().ok_or_else(|| format!("{display}:1: workspace members must be strings"))?;
            if member == "xtask" {
                continue;
            }
            let manifest = root.join(member).join("Cargo.toml");
            let (name, version) = read_package(&manifest.display().to_string(), &read(&manifest)?)?;
            clis.push(Cli { name, member: member.to_owned(), version });
        }
        Ok(Self { root: root.to_path_buf(), base, slug, clis })
    }

    /// Finds a CLI by package name.
    pub fn find(&self, name: &str) -> Result<&Cli, String> {
        self.clis.iter().find(|cli| cli.name == name).ok_or_else(|| {
            let names: Vec<&str> = self.clis.iter().map(|cli| cli.name.as_str()).collect();
            format!("unknown CLI `{name}`; expected one of: {}", names.join(", "))
        })
    }

    pub fn manifest_path(&self, cli: &Cli) -> PathBuf {
        self.root.join(&cli.member).join("Cargo.toml")
    }

    pub fn changelog_path(&self, cli: &Cli) -> PathBuf {
        self.root.join(&cli.member).join("CHANGELOG.md")
    }
}

/// Reads `[package] name` and the literal `[package] version` from a member manifest.
pub fn read_package(display: &str, text: &str) -> Result<(String, Version), String> {
    let document = parse(display, text)?;
    let package =
        table(document.as_table(), "package").ok_or_else(|| format!("{display}:1: missing [package] table"))?;
    let name = package
        .get("name")
        .and_then(Item::as_str)
        .ok_or_else(|| format!("{display}:1: missing [package] name"))?
        .to_owned();
    let (key, item) =
        package.get_key_value("version").ok_or_else(|| format!("{display}:1: missing [package] version"))?;
    let line = key.span().map_or(1, |span| line_of(text, span.start));
    let raw = item
        .as_str()
        .ok_or_else(|| format!("{display}:{line}: [package] version must be a literal string, not inherited"))?;
    let version = Version::parse(raw).map_err(|error| format!("{display}:{line}: invalid version `{raw}`: {error}"))?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        return Err(format!("{display}:{line}: version {version} must be a stable X.Y.Z release"));
    }
    Ok((name, version))
}

/// Replaces only the `[package] version` string in `text`, preserving every other byte.
pub fn rewrite_version(display: &str, text: &str, version: &Version) -> Result<String, String> {
    let span = version_span(display, text)?;
    let quote = &text[span.start..span.start + 1];
    Ok(format!("{}{quote}{version}{quote}{}", &text[..span.start], &text[span.end..]))
}

fn version_span(display: &str, text: &str) -> Result<Range<usize>, String> {
    let document = parse(display, text)?;
    let span = table(document.as_table(), "package")
        .and_then(|package| package.get("version"))
        .filter(|item| item.is_str())
        .and_then(Item::span)
        .ok_or_else(|| format!("{display}:1: missing literal [package] version string"))?;
    let raw = &text[span.clone()];
    let quoted = raw.len() >= 2 && (raw.starts_with('"') || raw.starts_with('\'')) && raw.ends_with(&raw[..1]);
    if !quoted || raw[1..].starts_with(&raw[..1]) {
        return Err(format!("{display}:{}: [package] version must be a single-line string", line_of(text, span.start)));
    }
    Ok(span)
}

fn read(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("{}:1: cannot read file: {error}", path.display()))
}

fn parse(display: &str, text: &str) -> Result<Document<String>, String> {
    Document::parse(text.to_owned()).map_err(|error| {
        let line = error.span().map_or(1, |span| line_of(text, span.start));
        format!("{display}:{line}: invalid TOML: {}", error.message())
    })
}

fn table<'a>(parent: &'a Table, key: &str) -> Option<&'a Table> {
    parent.get(key).and_then(Item::as_table)
}

fn line_of(text: &str, offset: usize) -> usize {
    text[..offset].matches('\n').count() + 1
}

fn github_slug(base: &str) -> Option<String> {
    let path = base.strip_prefix("https://github.com/")?;
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, repo) = path.split_once('/')?;
    (!owner.is_empty() && !repo.is_empty() && !repo.contains('/')).then(|| format!("{owner}/{repo}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "[package]\n# keep\nname = \"ai-demo\"   # trailing\nversion   =  \"1.0.0\" # pinned\n\n[dependencies]\nfoo = { version = \"1.0.0\" }\n";

    #[test]
    fn rewrite_changes_only_the_package_version() {
        let rewritten = rewrite_version("Cargo.toml", MANIFEST, &Version::new(1, 1, 0)).unwrap();
        assert_eq!(rewritten, MANIFEST.replacen("=  \"1.0.0\"", "=  \"1.1.0\"", 1));
        assert!(rewritten.contains("foo = { version = \"1.0.0\" }"));
    }

    #[test]
    fn rewrite_preserves_literal_quotes() {
        let text = "[package]\nname = \"ai-demo\"\nversion = '1.0.0'\n";
        let rewritten = rewrite_version("Cargo.toml", text, &Version::new(2, 0, 0)).unwrap();
        assert_eq!(rewritten, "[package]\nname = \"ai-demo\"\nversion = '2.0.0'\n");
    }

    #[test]
    fn inherited_version_is_rejected_with_its_line() {
        let text = "[package]\nname = \"ai-demo\"\nversion.workspace = true\n";
        let error = read_package("demo/Cargo.toml", text).unwrap_err();
        assert_eq!(error, "demo/Cargo.toml:3: [package] version must be a literal string, not inherited");
        assert!(rewrite_version("demo/Cargo.toml", text, &Version::new(1, 1, 0)).is_err());
    }

    #[test]
    fn reads_literal_package_version() {
        let (name, version) = read_package("Cargo.toml", MANIFEST).unwrap();
        assert_eq!((name.as_str(), version), ("ai-demo", Version::new(1, 0, 0)));
    }

    #[test]
    fn derives_github_slug() {
        assert_eq!(github_slug("https://github.com/PaulRBerg/agent-skills").as_deref(), Some("PaulRBerg/agent-skills"));
        assert_eq!(github_slug("https://example.com/a/b"), None);
    }
}
