//! Idempotent tag and GitHub release publication for one CLI.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use crate::{
    changelog::{Changelog, format_diagnostics},
    manifest::{self, Cli, Workspace},
};

/// Tags the latest release commit of `cli`, pushes the tag, and creates its GitHub release when missing.
pub fn publish(workspace: &Workspace, cli: &Cli) -> Result<(), String> {
    let toplevel = PathBuf::from(Git { dir: workspace.root.clone() }.text(&["rev-parse", "--show-toplevel"])?);
    // Run every later command from the toplevel so repository-relative pathspecs resolve.
    let git = Git { dir: toplevel.clone() };
    let changelog_path = repo_relative(&toplevel, &workspace.changelog_path(cli))?;
    let manifest_path = repo_relative(&toplevel, &workspace.manifest_path(cli))?;

    let upstream = git
        .text(&["rev-parse", "--abbrev-ref", "@{upstream}"])
        .map_err(|error| format!("the current branch needs an upstream branch: {error}"))?;
    let head = git.text(&["symbolic-ref", "-q", "HEAD"])?;
    let remote = git.text(&["for-each-ref", "--format=%(upstream:remotename)", &head])?;
    if remote.is_empty() {
        return Err(format!("cannot determine the remote of upstream {upstream}"));
    }
    git.run(&["fetch", "--quiet", &remote])?;

    let release = git.text(&["log", "-1", "--format=%H", "HEAD", "--", &changelog_path])?;
    if release.is_empty() {
        return Err(format!("no commit on HEAD touches {changelog_path}"));
    }
    let changelog_text = git.text_raw(&["show", &format!("{release}:{changelog_path}")])?;
    let manifest_text = git.text_raw(&["show", &format!("{release}:{manifest_path}")])?;
    let changelog = Changelog::parse(&changelog_text, &workspace.base, &cli.name).map_err(|diagnostics| {
        let lines = format_diagnostics(&changelog_path, &diagnostics).join("\n");
        format!("{changelog_path} is invalid at release commit {release}:\n{lines}")
    })?;
    let (name, manifest_version) = manifest::read_package(&format!("{manifest_path} at {release}"), &manifest_text)?;
    let version = changelog.top().version.clone();
    if name != cli.name || manifest_version != version {
        return Err(format!(
            "release commit {release} has {name} {manifest_version} in {manifest_path}, but its changelog's top release is {version}; run `just release-prepare {}` and commit the result",
            cli.name
        ));
    }

    match git.status(&["merge-base", "--is-ancestor", &release, "@{upstream}"])? {
        0 => {}
        1 => return Err(format!("release commit {release} is not on {upstream}; push the release commit first")),
        code => return Err(format!("git merge-base --is-ancestor failed with exit code {code}")),
    }

    let tag = format!("{}-v{version}", cli.name);
    let tag_ref = format!("refs/tags/{tag}");
    match git.try_text(&["rev-parse", "-q", "--verify", &format!("{tag_ref}^{{commit}}")]) {
        Some(commit) if commit != release => {
            return Err(format!("local tag {tag} points to {commit}, not release commit {release}"));
        }
        Some(_) => println!("Skipped: local tag {tag} already points to {release}."),
        None => {
            git.run(&["tag", "-a", &tag, &release, "-m", &format!("{} {version}", cli.name)])?;
            println!("Created: local tag {tag} at {release}.");
        }
    }

    let listing = git.text(&["ls-remote", "--tags", &remote, &tag_ref, &format!("{tag_ref}^{{}}")])?;
    match remote_commit(&listing, &tag_ref) {
        Some(commit) if commit != release => {
            return Err(format!("tag {tag} on {remote} points to {commit}, not release commit {release}"));
        }
        Some(_) => println!("Skipped: tag {tag} already exists on {remote}."),
        None => {
            git.run(&["push", "--quiet", &remote, &tag_ref])?;
            println!("Created: pushed tag {tag} to {remote}.");
        }
    }

    let view = output(Command::new("gh").args(["release", "view", &tag, "--repo", &workspace.slug]))?;
    if view.status.success() {
        println!("Skipped: GitHub release {tag} already exists on {}.", workspace.slug);
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&view.stderr);
    if !stderr.contains("release not found") {
        return Err(format!("gh release view {tag} failed: {}", stderr.trim()));
    }
    let notes = changelog.notes(&version).expect("the top release has notes");
    let notes_file = tempfile::NamedTempFile::new().map_err(|error| format!("cannot create notes file: {error}"))?;
    fs::write(notes_file.path(), notes).map_err(|error| format!("cannot write notes file: {error}"))?;
    let title = format!("{} {version}", cli.name);
    let created = output(
        Command::new("gh")
            .args(["release", "create", &tag, "--repo", &workspace.slug, "--verify-tag", "--title", &title])
            .arg("--notes-file")
            .arg(notes_file.path())
            .arg("--latest=false"),
    )?;
    if !created.status.success() {
        return Err(format!("gh release create {tag} failed: {}", String::from_utf8_lossy(&created.stderr).trim()));
    }
    println!("Created: GitHub release {tag} on {}.", workspace.slug);
    Ok(())
}

/// The commit a remote tag points to, preferring the peeled `^{}` line of annotated tags.
fn remote_commit(listing: &str, tag_ref: &str) -> Option<String> {
    let entries: Vec<(&str, &str)> = listing.lines().filter_map(|line| line.split_once('\t')).collect();
    let peeled = format!("{tag_ref}^{{}}");
    let find = |name: &str| entries.iter().find(|(_, reference)| *reference == name).map(|(sha, _)| (*sha).to_owned());
    find(&peeled).or_else(|| find(tag_ref))
}

fn repo_relative(toplevel: &Path, path: &Path) -> Result<String, String> {
    let canonical =
        |path: &Path| path.canonicalize().map_err(|error| format!("cannot resolve {}: {error}", path.display()));
    let toplevel = canonical(toplevel)?;
    let parent = path.parent().expect("member files have a parent directory");
    let file = path.file_name().expect("member files have a name");
    let path = canonical(parent)?.join(file);
    let relative = path
        .strip_prefix(&toplevel)
        .map_err(|_| format!("{} is outside the Git repository {}", path.display(), toplevel.display()))?;
    let parts: Vec<String> =
        relative.components().map(|part| part.as_os_str().to_string_lossy().into_owned()).collect();
    Ok(parts.join("/"))
}

fn output(command: &mut Command) -> Result<Output, String> {
    let program = command.get_program().to_string_lossy().into_owned();
    command.output().map_err(|error| format!("cannot run {program}: {error}"))
}

struct Git {
    dir: PathBuf,
}

impl Git {
    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new("git");
        command.arg("-C").arg(&self.dir).args(args);
        command
    }

    fn run(&self, args: &[&str]) -> Result<Output, String> {
        let result = output(&mut self.command(args))?;
        if result.status.success() {
            Ok(result)
        } else {
            Err(format!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&result.stderr).trim()))
        }
    }

    fn text_raw(&self, args: &[&str]) -> Result<String, String> {
        String::from_utf8(self.run(args)?.stdout).map_err(|_| format!("git {} printed invalid UTF-8", args.join(" ")))
    }

    fn text(&self, args: &[&str]) -> Result<String, String> {
        Ok(self.text_raw(args)?.trim().to_owned())
    }

    fn try_text(&self, args: &[&str]) -> Option<String> {
        self.text(args).ok()
    }

    fn status(&self, args: &[&str]) -> Result<i32, String> {
        let result = output(&mut self.command(args))?;
        result.status.code().ok_or_else(|| format!("git {} was terminated by a signal", args.join(" ")))
    }
}

#[cfg(test)]
mod tests {
    use super::remote_commit;

    #[test]
    fn remote_commit_prefers_peeled_line() {
        let listing = "aaa\trefs/tags/ai-demo-v1.0.0\nbbb\trefs/tags/ai-demo-v1.0.0^{}\n";
        assert_eq!(remote_commit(listing, "refs/tags/ai-demo-v1.0.0").as_deref(), Some("bbb"));
        assert_eq!(
            remote_commit("ccc\trefs/tags/ai-demo-v1.0.0\n", "refs/tags/ai-demo-v1.0.0").as_deref(),
            Some("ccc")
        );
        assert_eq!(remote_commit("", "refs/tags/ai-demo-v1.0.0"), None);
    }
}
