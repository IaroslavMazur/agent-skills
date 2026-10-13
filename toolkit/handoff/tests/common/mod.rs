#![allow(dead_code)]

use std::{
    env,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::{Command as ProcessCommand, Output},
};

use assert_cmd::Command;
use tempfile::TempDir;

pub struct Harness {
    _temporary: TempDir,
    pub root: PathBuf,
    pub home: PathBuf,
    pub desktop: PathBuf,
    pub clipboard: PathBuf,
    shim: PathBuf,
    search_path: String,
}

impl Harness {
    pub fn new(name: &str) -> Self {
        let temporary = tempfile::Builder::new().prefix(&format!("ai-handoff-{name}-")).tempdir().unwrap();
        let root = temporary.path().canonicalize().unwrap();
        let home = root.join("home");
        let desktop = home.join("Desktop");
        let shim = root.join("shim");
        let clipboard = root.join("clipboard");
        fs::create_dir_all(&desktop).unwrap();
        fs::create_dir_all(&shim).unwrap();
        write_executable(&shim.join("pbcopy"), "#!/bin/sh\n/bin/cat > \"$HANDOFF_CLIPBOARD\"\n");
        write_executable(&shim.join("pbpaste"), "#!/bin/sh\n/bin/cat \"$HANDOFF_CLIPBOARD\"\n");
        write_executable(
            &shim.join("wl-copy"),
            "#!/bin/sh\n[ \"$*\" = '--type text/plain' ] || exit 2\n/bin/cat > \"$HANDOFF_CLIPBOARD\"\n",
        );
        write_executable(
            &shim.join("wl-paste"),
            "#!/bin/sh\n[ \"$*\" = '--no-newline --type text/plain' ] || exit 2\n/bin/cat \"$HANDOFF_CLIPBOARD\"\n",
        );
        write_executable(
            &shim.join("xclip"),
            "#!/bin/sh\ncase \"$*\" in\n'-selection clipboard -in') /bin/cat > \"$HANDOFF_CLIPBOARD\";;\n'-selection clipboard -out') /bin/cat \"$HANDOFF_CLIPBOARD\";;\n*) exit 2;;\nesac\n",
        );
        std::os::unix::fs::symlink(git_executable(), shim.join("git")).unwrap();
        let search_path = shim.display().to_string();
        Self { _temporary: temporary, root, home, desktop, clipboard, shim, search_path }
    }

    pub fn repo(&self, name: &str, ignore_handoffs: bool) -> PathBuf {
        let repository = self.root.join(name);
        fs::create_dir(&repository).unwrap();
        let output = ProcessCommand::new("git")
            .arg("-C")
            .arg(&repository)
            .args(["init", "--quiet"])
            .env("HOME", &self.home)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
        assert!(output.status.success(), "git init failed: {}", String::from_utf8_lossy(&output.stderr));
        if ignore_handoffs {
            fs::write(repository.join(".gitignore"), ".ai/\n").unwrap();
        }
        repository
    }

    pub fn command<I, S>(&self, arguments: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.command_with_path(arguments, &self.search_path, &[])
    }

    pub fn command_with_env<I, S>(&self, arguments: I, environment: &[(&str, &str)]) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.command_with_path(arguments, &self.search_path, environment)
    }

    pub fn clipboard_reader(&self) -> PathBuf {
        self.shim.join(if cfg!(target_os = "linux") { "wl-paste" } else { "pbpaste" })
    }

    pub fn clipboard_writer(&self) -> PathBuf {
        self.shim.join(if cfg!(target_os = "linux") { "wl-copy" } else { "pbcopy" })
    }

    pub fn command_without_clipboard<I, S>(&self, arguments: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let bin = self.root.join("without-clipboard");
        fs::create_dir_all(&bin).unwrap();
        let git = git_executable();
        #[cfg(unix)]
        std::os::unix::fs::symlink(git, bin.join("git")).unwrap();
        self.command_with_path(arguments, &bin.display().to_string(), &[])
    }

    fn command_with_path<I, S>(&self, arguments: I, search_path: &str, environment: &[(&str, &str)]) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::cargo_bin("ai-handoff").expect("binary should be built for integration tests");
        command
            .current_dir(&self.root)
            .env("HOME", &self.home)
            .env("HANDOFF_CLIPBOARD", &self.clipboard)
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env("PATH", search_path)
            .env("WAYLAND_DISPLAY", "ai-handoff-test")
            .env("DISPLAY", "")
            .envs(environment.iter().copied())
            .args(arguments);
        command.output().unwrap()
    }
}

pub fn write_executable(path: &Path, contents: &str) {
    fs::write(path, contents).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

pub fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn git_executable() -> PathBuf {
    env::split_paths(&env::var_os("PATH").unwrap_or_default())
        .map(|directory| directory.join("git"))
        .find(|candidate| candidate.is_file())
        .expect("git is available on PATH")
}
