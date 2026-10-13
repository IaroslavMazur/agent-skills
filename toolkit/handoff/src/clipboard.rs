use std::{
    io::Write,
    process::{Command, Stdio},
};

use crate::error::{Error, Result};

type ClipboardCommand = (&'static str, &'static [&'static str]);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Backend {
    #[cfg(any(target_os = "macos", test))]
    MacOs,
    #[cfg(any(target_os = "linux", test))]
    Wayland,
    #[cfg(any(target_os = "linux", test))]
    X11,
}

impl Backend {
    fn commands(self) -> (ClipboardCommand, ClipboardCommand) {
        match self {
            #[cfg(any(target_os = "macos", test))]
            Self::MacOs => (("pbcopy", &[]), ("pbpaste", &[])),
            #[cfg(any(target_os = "linux", test))]
            Self::Wayland => {
                (("wl-copy", &["--type", "text/plain"]), ("wl-paste", &["--no-newline", "--type", "text/plain"]))
            }
            #[cfg(any(target_os = "linux", test))]
            Self::X11 => {
                (("xclip", &["-selection", "clipboard", "-in"]), ("xclip", &["-selection", "clipboard", "-out"]))
            }
        }
    }
}

pub(crate) fn copy_and_verify(text: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    let backend = Backend::MacOs;
    #[cfg(target_os = "linux")]
    let backend = select_linux(
        std::env::var_os("WAYLAND_DISPLAY").is_some_and(|value| !value.is_empty()),
        std::env::var_os("DISPLAY").is_some_and(|value| !value.is_empty()),
        executable_available,
    )?;
    copy_with_backend(text, backend)
}

#[cfg(any(target_os = "linux", test))]
fn select_linux(wayland: bool, x11: bool, executable: impl Fn(&str) -> bool) -> Result<Backend> {
    if wayland && executable("wl-copy") && executable("wl-paste") {
        return Ok(Backend::Wayland);
    }
    if x11 && executable("xclip") {
        return Ok(Backend::X11);
    }
    Err(Error::operational(
        "clipboard is unavailable: Linux requires WAYLAND_DISPLAY with executable wl-copy and wl-paste (wl-clipboard), or DISPLAY with executable xclip; use --no-clipboard for headless creation",
    ))
}

#[cfg(target_os = "linux")]
fn executable_available(name: &str) -> bool {
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).any(|directory| {
        let candidate = directory.join(name);
        candidate.is_file() && rustix::fs::access(&candidate, rustix::fs::Access::EXEC_OK).is_ok()
    })
}

fn copy_with_backend(text: &str, backend: Backend) -> Result<()> {
    let ((writer, write_args), (reader, read_args)) = backend.commands();
    let mut write_command = Command::new(writer);
    write_command.args(write_args);
    let mut read_command = Command::new(reader);
    read_command.args(read_args);
    copy_with_commands(text, write_command, read_command)
}

fn copy_with_commands(text: &str, mut writer: Command, mut reader: Command) -> Result<()> {
    let writer_name = writer.get_program().to_string_lossy().into_owned();
    let reader_name = reader.get_program().to_string_lossy().into_owned();
    let mut child = writer
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        // Clipboard owners can daemonize and inherit stderr. A pipe would wait for their EOF.
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| Error::operational(format!("required executable {writer_name} is unavailable: {error}; use --no-clipboard to skip clipboard copying")))?;
    let write_result = child.stdin.take().expect("piped clipboard stdin").write_all(text.as_bytes());
    if let Err(error) = write_result {
        let _ = child.kill();
        let _ = child.wait();
        return Err(Error::operational(format!("clipboard copy failed: {error}")));
    }
    let copied = child.wait().map_err(|error| Error::operational(format!("clipboard copy failed: {error}")))?;
    if !copied.success() {
        return Err(Error::operational("clipboard copy failed"));
    }

    let pasted = reader.output().map_err(|error| {
        Error::operational(format!(
            "required executable {reader_name} is unavailable: {error}; use --no-clipboard to skip clipboard copying"
        ))
    })?;
    if !pasted.status.success() {
        return Err(Error::operational("clipboard readback failed"));
    }
    if pasted.stdout != text.as_bytes() {
        return Err(Error::operational("clipboard verification failed"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt, path::Path};

    fn executable(path: &Path, script: &str) {
        fs::write(path, script).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn mock_commands(directory: &Path, backend: Backend) -> (Command, Command) {
        let ((_, write_args), (_, read_args)) = backend.commands();
        let writer = directory.join("writer");
        let reader = directory.join("reader");
        executable(
            &writer,
            &format!("#!/bin/sh\n[ \"$*\" = '{}' ] || exit 2\n/bin/cat > \"$CLIPBOARD\"\n", write_args.join(" ")),
        );
        executable(
            &reader,
            &format!("#!/bin/sh\n[ \"$*\" = '{}' ] || exit 2\n/bin/cat \"$CLIPBOARD\"\n", read_args.join(" ")),
        );
        let mut write_command = Command::new(writer);
        write_command.args(write_args).env("CLIPBOARD", directory.join("clipboard"));
        let mut read_command = Command::new(reader);
        read_command.args(read_args).env("CLIPBOARD", directory.join("clipboard"));
        (write_command, read_command)
    }

    #[test]
    fn every_backend_passes_arguments_and_verifies_exact_bytes_without_a_real_clipboard() {
        for backend in [Backend::MacOs, Backend::Wayland, Backend::X11] {
            let directory = tempfile::tempdir().unwrap();
            let (writer, reader) = mock_commands(directory.path(), backend);
            let text = "codex 'quotes' café\nsecond line\n";
            copy_with_commands(text, writer, reader).unwrap();
            assert_eq!(fs::read(directory.path().join("clipboard")).unwrap(), text.as_bytes());
        }
    }

    #[test]
    fn clipboard_owner_stderr_does_not_delay_readback() {
        let directory = tempfile::tempdir().unwrap();
        let (mut writer, mut reader) = mock_commands(directory.path(), Backend::Wayland);
        executable(
            &directory.path().join("writer"),
            "#!/bin/sh\n/bin/sleep 5 &\nprintf '%s' \"$!\" > \"$OWNER_PID\"\n/bin/cat > \"$CLIPBOARD\"\n",
        );
        executable(
            &directory.path().join("reader"),
            "#!/bin/sh\nkill \"$(/bin/cat \"$OWNER_PID\")\"\n/bin/cat \"$CLIPBOARD\"\n",
        );
        writer.env("OWNER_PID", directory.path().join("owner-pid"));
        reader.env("OWNER_PID", directory.path().join("owner-pid"));
        let started = std::time::Instant::now();
        copy_with_commands("clipboard owner test", writer, reader).unwrap();
        assert!(started.elapsed() < std::time::Duration::from_secs(4));
    }

    #[test]
    fn linux_prefers_wayland_and_requires_both_tools() {
        assert_eq!(select_linux(true, true, |_| true).unwrap(), Backend::Wayland);
        for missing in ["wl-copy", "wl-paste"] {
            assert_eq!(select_linux(true, true, |name| name != missing).unwrap(), Backend::X11);
            assert!(select_linux(true, false, |name| name != missing).is_err());
        }
    }

    #[test]
    fn linux_requires_a_nonempty_session_and_executable_tools() {
        assert_eq!(select_linux(false, true, |_| true).unwrap(), Backend::X11);
        assert!(select_linux(false, false, |_| true).unwrap_err().to_string().contains("--no-clipboard"));
        assert!(select_linux(true, true, |_| false).is_err());
        assert!(select_linux(false, true, |name| name != "xclip").is_err());
    }

    #[test]
    fn clipboard_arguments_preserve_plain_text_and_exact_readback() {
        assert_eq!(Backend::MacOs.commands(), (("pbcopy", &[][..]), ("pbpaste", &[][..])));
        assert_eq!(
            Backend::Wayland.commands(),
            (("wl-copy", &["--type", "text/plain"][..]), ("wl-paste", &["--no-newline", "--type", "text/plain"][..]))
        );
        assert_eq!(
            Backend::X11.commands(),
            (("xclip", &["-selection", "clipboard", "-in"][..]), ("xclip", &["-selection", "clipboard", "-out"][..]))
        );
    }
}
