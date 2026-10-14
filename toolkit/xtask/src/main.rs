//! Release tooling for the toolkit CLIs: changelog validation, version preparation, notes, and publication.

mod changelog;
mod manifest;
mod publish;

use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use clap::{Parser, Subcommand};
use semver::Version;

use crate::{
    changelog::{Changelog, expected_next, format_diagnostics, step},
    manifest::{Cli, Workspace},
};

#[derive(Parser)]
#[command(about = "Release tooling for the toolkit CLIs")]
struct Args {
    /// Workspace root containing the toolkit Cargo.toml.
    #[arg(long, global = true, hide = true, value_name = "PATH", default_value_os_t = default_root())]
    workspace_root: PathBuf,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Validate every CLI changelog against its manifest version.
    Check,
    /// Sync a CLI's manifest and Cargo.lock to its newest changelog entry.
    Prepare {
        /// CLI package name, for example `ai-coord`.
        #[arg(value_name = "CRATE")]
        krate: String,
    },
    /// Print the release notes for a changelog entry.
    Notes {
        /// CLI package name, for example `ai-coord`.
        #[arg(value_name = "CRATE")]
        krate: String,
        /// Release version; defaults to the newest release.
        #[arg(long)]
        version: Option<Version>,
    },
    /// Tag the release commit, push the tag, and create the GitHub release.
    Publish {
        /// CLI package name, for example `ai-coord`.
        #[arg(value_name = "CRATE")]
        krate: String,
    },
}

fn default_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("xtask lives inside the workspace").to_path_buf()
}

fn main() -> ExitCode {
    let args = Args::parse();
    let result = Workspace::load(&args.workspace_root).and_then(|workspace| match args.command {
        Commands::Check => check(&workspace),
        Commands::Prepare { krate } => prepare(&workspace, &krate),
        Commands::Notes { krate, version } => notes(&workspace, &krate, version),
        Commands::Publish { krate } => workspace.find(&krate).and_then(|cli| publish::publish(&workspace, cli)),
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

/// Reads and validates one CLI changelog, returning formatted error lines on failure.
fn load_changelog(workspace: &Workspace, cli: &Cli) -> Result<Changelog, String> {
    let path = workspace.changelog_path(cli);
    let display = display(&path);
    let text = fs::read_to_string(&path).map_err(|error| format!("{display}:1: cannot read changelog: {error}"))?;
    Changelog::parse(&text, &workspace.base, &cli.name)
        .map_err(|diagnostics| format_diagnostics(&display, &diagnostics).join("\n"))
}

fn check(workspace: &Workspace) -> Result<(), String> {
    let mut errors = Vec::new();
    for cli in &workspace.clis {
        let checked = load_changelog(workspace, cli).and_then(|changelog| {
            let top = changelog.top();
            if top.version == cli.version {
                Ok(())
            } else {
                Err(format!(
                    "{}:{}: top release {} does not match the {} manifest version {} in {}",
                    display(&workspace.changelog_path(cli)),
                    top.line,
                    top.version,
                    cli.name,
                    cli.version,
                    display(&workspace.manifest_path(cli)),
                ))
            }
        });
        match checked {
            Ok(()) => println!("OK {} {} ({})", cli.name, cli.version, display(&workspace.changelog_path(cli))),
            Err(error) => errors.push(error),
        }
    }
    if errors.is_empty() { Ok(()) } else { Err(errors.join("\n")) }
}

fn prepare(workspace: &Workspace, name: &str) -> Result<(), String> {
    let cli = workspace.find(name)?;
    let changelog = load_changelog(workspace, cli)?;
    let top = changelog.top();
    let changelog_display = display(&workspace.changelog_path(cli));
    if top.version == cli.version {
        return Err(format!(
            "{changelog_display}:{}: top release {} equals the {name} manifest version; add the new release entry to the changelog first",
            top.line, top.version
        ));
    }
    if step(&cli.version, &top.version).is_none() {
        return Err(format!(
            "{changelog_display}:{}: {} is not a valid next version after the {name} manifest version {}; expected {}",
            top.line,
            top.version,
            cli.version,
            expected_next(&cli.version)
        ));
    }

    let manifest_path = workspace.manifest_path(cli);
    let manifest_display = display(&manifest_path);
    let lock_path = workspace.root.join("Cargo.lock");
    let lock_display = display(&lock_path);
    let text = fs::read_to_string(&manifest_path).map_err(|error| format!("{manifest_display}:1: {error}"))?;
    let lock = fs::read_to_string(&lock_path).map_err(|error| format!("{lock_display}:1: {error}"))?;
    let rewritten = manifest::rewrite_version(&manifest_display, &text, &top.version)?;
    fs::write(&manifest_path, rewritten).map_err(|error| format!("cannot write {manifest_display}: {error}"))?;
    if let Err(error) = sync_lock(&workspace.root, name) {
        // Restore both files so that a rerun starts from the unprepared state.
        let restored = fs::write(&manifest_path, &text).and_then(|()| fs::write(&lock_path, &lock));
        return Err(match restored {
            Ok(()) => format!("{error}\nRestored {manifest_display} and {lock_display}."),
            Err(restore) => format!("{error}\ncannot restore {manifest_display} and {lock_display}: {restore}"),
        });
    }
    println!("Updated {manifest_display}: {name} {} -> {}", cli.version, top.version);
    println!("Updated {lock_display}: {name} {}", top.version);

    check(&Workspace::load(&workspace.root)?)?;
    println!(
        "Next steps:\n  1. Review and commit {changelog_display}, {manifest_display}, and {lock_display} in one release commit.\n  2. Push the release commit to the upstream branch.\n  3. Run `just release-publish {name}`."
    );
    Ok(())
}

/// Updates only `name`'s Cargo.lock entry, then confirms the lock matches the manifests.
fn sync_lock(root: &Path, name: &str) -> Result<(), String> {
    let workspace_manifest = root.join("Cargo.toml");
    cargo(&[
        "update".into(),
        "--package".into(),
        name.into(),
        "--offline".into(),
        "--quiet".into(),
        "--manifest-path".into(),
        workspace_manifest.clone().into(),
    ])?;
    let host = host_triple()?;
    cargo(&[
        "metadata".into(),
        "--locked".into(),
        "--offline".into(),
        "--format-version".into(),
        "1".into(),
        "--filter-platform".into(),
        host.into(),
        "--manifest-path".into(),
        workspace_manifest.into(),
    ])?;
    Ok(())
}

fn notes(workspace: &Workspace, name: &str, version: Option<Version>) -> Result<(), String> {
    let cli = workspace.find(name)?;
    let changelog = load_changelog(workspace, cli)?;
    let version = version.unwrap_or_else(|| changelog.top().version.clone());
    let notes = changelog
        .notes(&version)
        .ok_or_else(|| format!("{}: no release {version}", display(&workspace.changelog_path(cli))))?;
    print!("{notes}");
    Ok(())
}

/// Runs the cargo that launched this process, or `cargo` from PATH, and returns its stdout.
fn cargo(args: &[OsString]) -> Result<String, String> {
    let program = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let rendered: Vec<String> = args.iter().map(|arg| arg.to_string_lossy().into_owned()).collect();
    let output = Command::new(&program)
        .args(args)
        .output()
        .map_err(|error| format!("cannot run {}: {error}", program.to_string_lossy()))?;
    if !output.status.success() {
        return Err(format!(
            "cargo {} failed:\n{}",
            rendered.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|_| format!("cargo {} printed invalid UTF-8", rendered.join(" ")))
}

/// The host target triple, which keeps `cargo metadata` from needing packages for other platforms.
fn host_triple() -> Result<String, String> {
    let version = cargo(&["-vV".into()])?;
    version
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(str::to_owned)
        .ok_or_else(|| "cargo -vV did not report a host triple".to_owned())
}

/// Shows `path` relative to the current directory when it is inside it.
fn display(path: &Path) -> String {
    env::current_dir()
        .ok()
        .and_then(|cwd| path.strip_prefix(cwd).ok().map(Path::to_path_buf))
        .unwrap_or_else(|| path.to_path_buf())
        .display()
        .to_string()
}
