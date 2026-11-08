//! Brings a running Emacs up to date after `emx sync`, `emx upgrade` or `emx
//! rollback` has switched emx's profile. The new generation's `emacsclient`
//! asks this user's Emacs server to run `emx-reload-write-report`
//! (lisp/emx-reload.el), which writes how the reload went as JSON to a file
//! this module names, so nothing here reads what Emacs prints. Without a
//! server, there is nothing to reload.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::daemon;
use crate::profile::Profile;

/// How long Emacs gets to reload. A reload that loads every package again
/// takes several seconds, so an Emacs still reloading after this long is
/// likely stuck, as on a prompt.
const TIMEOUT: Duration = Duration::from_secs(30);

/// What `emx-reload` did, as `emx-reload-write-report` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum Outcome {
    /// Emacs runs the new generation and configuration.
    Reloaded,
    /// Emacs runs them, but keeps what they removed, or something failed.
    RestartRecommended,
    /// Emacs can't load the generation in place, and nothing changed.
    RestartRequired,
    /// The Emacs serving the socket doesn't run emx's configuration, so there
    /// is nothing to reload.
    NotEmx,
}

/// The file `emx-reload-write-report` writes.
#[derive(Debug, Deserialize)]
struct Report {
    outcome: Outcome,
    /// What happened, in lines for the user.
    report: String,
    /// Whether the Emacs that answered is a daemon, which decides how to
    /// restart it.
    #[serde(default)]
    daemon: bool,
}

/// What the expression writes when the server isn't emx's Emacs, which has
/// no `emx-reload` to write a report.
const NOT_EMX: &str = r#"{"outcome": "not-emx", "report": "emx: the running Emacs doesn't run emx's configuration, so there is nothing to reload"}"#;

/// Why a running Emacs didn't reload.
#[derive(Debug, thiserror::Error)]
pub enum ReloadError {
    #[error("cannot reach its server {path}: {source}")]
    Unreachable { path: PathBuf, source: io::Error },

    #[error("cannot name {path} to Emacs: it is not UTF-8")]
    ReportPath { path: PathBuf },

    #[error("cannot run {path}: {source}")]
    Run { path: PathBuf, source: io::Error },

    #[error("emacsclient failed ({status}); its output is above")]
    Failed { status: ExitStatus },

    /// emx stops waiting, but Emacs goes on with the reload.
    #[error(
        "the running Emacs hasn't finished reloading within {} s: it may still be \
         reloading, or be stuck, as on a prompt",
        TIMEOUT.as_secs()
    )]
    TimedOut,

    #[error("cannot read its report {path}: {source}")]
    ReadReport { path: PathBuf, source: io::Error },

    #[error("cannot remove the report {path}: {source}")]
    RemoveReport { path: PathBuf, source: io::Error },

    #[error("cannot parse its report {path}: {source}")]
    ParseReport {
        path: PathBuf,
        source: serde_json::Error,
    },
}

/// Reloads this user's running Emacs, if one serves, into the current
/// generation of `profile`, and prints its report.
pub fn running_emacs(profile: &Profile) -> Result<(), ReloadError> {
    let socket = server_socket(
        std::env::var_os("EMACS_SOCKET_NAME"),
        std::env::var_os("XDG_RUNTIME_DIR"),
        std::env::var_os("TMPDIR"),
        uid(),
    );
    if !serves(&socket)? {
        return Ok(());
    }
    let file = profile.reload_report();
    remove(&file)?;
    let emacsclient = profile.link().join("bin/emacsclient");
    let mut socket_arg = OsString::from("--socket-name=");
    socket_arg.push(&socket);
    let run_error = |source| ReloadError::Run {
        path: emacsclient.clone(),
        source,
    };
    let mut child = Command::new(&emacsclient)
        .arg(socket_arg)
        .arg("--eval")
        .arg(expression(&file, &profile.manifest())?)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .map_err(run_error)?;
    let status = wait(&mut child, TIMEOUT)
        .map_err(run_error)?
        .ok_or(ReloadError::TimedOut)?;
    if !status.success() {
        return Err(ReloadError::Failed { status });
    }

    let report = read_report(&file)?;
    println!("{}", report.report);
    if matches!(
        report.outcome,
        Outcome::RestartRecommended | Outcome::RestartRequired
    ) {
        daemon::print_restart(report.daemon);
    }
    Ok(())
}

/// The socket `emacsclient` reaches by default, by the rules server.el makes
/// it with. `socket_name` (EMACS_SOCKET_NAME) is a file name when it holds a
/// `/`, and otherwise a name in the socket directory: `$XDG_RUNTIME_DIR/emacs`,
/// or `emacs<uid>` in `$TMPDIR` or `/tmp`.
fn server_socket(
    socket_name: Option<OsString>,
    runtime_dir: Option<OsString>,
    tmpdir: Option<OsString>,
    uid: u32,
) -> PathBuf {
    let set = |value: Option<OsString>| value.filter(|value| !value.is_empty());
    let name = set(socket_name).unwrap_or_else(|| "server".into());
    if name.as_encoded_bytes().contains(&b'/') {
        return name.into();
    }
    let dir = match set(runtime_dir) {
        Some(runtime_dir) => PathBuf::from(runtime_dir).join("emacs"),
        None => {
            PathBuf::from(set(tmpdir).unwrap_or_else(|| "/tmp".into())).join(format!("emacs{uid}"))
        }
    };
    dir.join(name)
}

fn uid() -> u32 {
    // SAFETY: getuid has no preconditions and cannot fail.
    unsafe { libc::getuid() }
}

/// Whether an Emacs server listens on `socket`. A socket file left by an
/// Emacs that died refuses the connection.
fn serves(socket: &Path) -> Result<bool, ReloadError> {
    match UnixStream::connect(socket) {
        Ok(_) => Ok(true),
        Err(err)
            if matches!(
                err.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
            ) =>
        {
            Ok(false)
        }
        Err(source) => Err(ReloadError::Unreachable {
            path: socket.to_owned(),
            source,
        }),
    }
}

/// The Elisp that has the server reload into `manifest` and report to `file`.
/// It loads emx-reload from the emx repo that Emacs's configuration came from,
/// since an Emacs started before emx-reload existed doesn't have it. An Emacs
/// without emx's configuration has no repo, and reports that instead.
fn expression(file: &Path, manifest: &Path) -> Result<String, ReloadError> {
    fn utf8(path: &Path) -> Result<&str, ReloadError> {
        path.to_str().ok_or_else(|| ReloadError::ReportPath {
            path: path.to_owned(),
        })
    }
    let file = elisp_string(utf8(file)?);
    Ok(format!(
        "(if (boundp 'emx-source-dir) \
         (progn (load (expand-file-name \"lisp/emx-reload\" emx-source-dir) nil t) \
         (emx-reload-write-report {file} {})) \
         (with-temp-file {file} (insert {})))",
        elisp_string(utf8(manifest)?),
        elisp_string(NOT_EMX)
    ))
}

/// `text` as an Elisp string literal.
fn elisp_string(text: &str) -> String {
    let mut literal = String::with_capacity(text.len() + 2);
    literal.push('"');
    for c in text.chars() {
        if matches!(c, '"' | '\\') {
            literal.push('\\');
        }
        literal.push(c);
    }
    literal.push('"');
    literal
}

/// Waits for `child` for up to `timeout`, then kills it. Returns its status,
/// or `None` when it ran out of time. emacsclient's own `--timeout` doesn't
/// cover an evaluation that never ends.
fn wait(child: &mut Child, timeout: Duration) -> io::Result<Option<ExitStatus>> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn remove(file: &Path) -> Result<(), ReloadError> {
    match fs::remove_file(file) {
        Err(err) if err.kind() != io::ErrorKind::NotFound => Err(ReloadError::RemoveReport {
            path: file.to_owned(),
            source: err,
        }),
        _ => Ok(()),
    }
}

/// Reads and removes the report in `file`.
fn read_report(file: &Path) -> Result<Report, ReloadError> {
    let text = fs::read_to_string(file).map_err(|source| ReloadError::ReadReport {
        path: file.to_owned(),
        source,
    })?;
    remove(file)?;
    serde_json::from_str(&text).map_err(|source| ReloadError::ParseReport {
        path: file.to_owned(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(value: &str) -> Option<OsString> {
        Some(value.into())
    }

    #[test]
    fn reads_the_report_written_for_an_emacs_without_emx() {
        let report: Report = serde_json::from_str(NOT_EMX).unwrap();
        assert_eq!(report.outcome, Outcome::NotEmx);
        assert!(!report.daemon);
    }

    #[test]
    fn finds_the_socket_in_the_runtime_directory() {
        let socket = server_socket(None, os("/run/user/1000"), os("/tmp/x"), 1000);
        assert_eq!(socket, Path::new("/run/user/1000/emacs/server"));
    }

    #[test]
    fn finds_the_socket_in_the_temporary_directory_without_a_runtime_one() {
        assert_eq!(
            server_socket(None, None, os("/var/tmp"), 1000),
            Path::new("/var/tmp/emacs1000/server")
        );
        assert_eq!(
            server_socket(None, os(""), None, 7),
            Path::new("/tmp/emacs7/server")
        );
    }

    #[test]
    fn takes_the_socket_name_from_the_environment() {
        let dir = os("/run/user/1000");
        assert_eq!(
            server_socket(os("work"), dir.clone(), None, 1000),
            Path::new("/run/user/1000/emacs/work")
        );
        assert_eq!(
            server_socket(os("/srv/emacs.sock"), dir, None, 1000),
            Path::new("/srv/emacs.sock")
        );
    }

    #[test]
    fn quotes_file_names_for_elisp() {
        assert_eq!(elisp_string(r#"/a "b"\c"#), r#""/a \"b\"\\c""#);
    }

    #[test]
    fn reads_each_outcome() {
        for (name, outcome) in [
            ("reloaded", Outcome::Reloaded),
            ("restart-recommended", Outcome::RestartRecommended),
            ("restart-required", Outcome::RestartRequired),
        ] {
            let text = format!(r#"{{"outcome": "{name}", "report": "r"}}"#);
            let report: Report = serde_json::from_str(&text).unwrap();
            assert_eq!(report.outcome, outcome);
        }
    }

    #[test]
    fn stops_waiting_at_the_timeout() {
        let mut child = Command::new("sleep").arg("10").spawn().unwrap();
        let started = Instant::now();
        assert_eq!(wait(&mut child, Duration::from_millis(100)).unwrap(), None);
        assert!(started.elapsed() < Duration::from_secs(5));
        // Killed and reaped.
        assert!(child.try_wait().unwrap().is_some());
    }

    #[test]
    fn returns_the_status_of_a_child_that_finishes() {
        let mut child = Command::new("true").spawn().unwrap();
        let status = wait(&mut child, Duration::from_secs(10)).unwrap().unwrap();
        assert!(status.success());
    }
}
