//! The Emacs daemon that nix/home-module.nix runs from emx's profile: how to
//! restart it, and starting it after the sync that creates the profile.
//! home-manager's `emacs.service` skips while the profile is missing, so a
//! session that started before that sync started no daemon. The module names
//! the daemon's unit in `EMX_DAEMON_UNIT` whenever it runs the daemon, and
//! sets `EMX_DAEMON_STARTS_WITH_SESSION` when the session starts it, so that
//! is the whole decision: systemd is asked to start the unit, never what state
//! it is in.

use std::ffi::{OsStr, OsString};
use std::io;
use std::process::{Command, Stdio};

/// Set by nix/home-module.nix to the unit that runs the Emacs daemon.
const UNIT_VAR: &str = "EMX_DAEMON_UNIT";

/// Set to `1` by nix/home-module.nix when the session starts that unit.
const WITH_SESSION_VAR: &str = "EMX_DAEMON_STARTS_WITH_SESSION";

/// What the sync that creates the profile did about the daemon.
#[derive(Debug, PartialEq, Eq)]
pub enum Start {
    /// The session's daemon unit started.
    Started,
    /// Nothing was started, for the reason given.
    Skipped(&'static str),
}

/// Tells the user how to restart Emacs, for a generation that a running Emacs
/// can't wholly load in place (reload.rs). `daemon` says whether the Emacs
/// serving the socket is a daemon.
pub fn print_restart(daemon: bool) {
    println!("emx: {}", restart_advice(daemon, daemon_unit().as_deref()));
}

/// How to restart Emacs: through systemd only for a daemon when the daemon's
/// unit is known, since a standalone Emacs serving the socket isn't that unit.
fn restart_advice(daemon: bool, unit: Option<&OsStr>) -> String {
    match (daemon, unit) {
        (true, Some(unit)) => format!(
            "restart Emacs: systemctl --user restart {}",
            unit.to_string_lossy()
        ),
        (true, None) => "restart the Emacs daemon".to_owned(),
        (false, _) => "restart Emacs".to_owned(),
    }
}

/// The unit that runs the Emacs daemon, if there is one.
fn daemon_unit() -> Option<OsString> {
    std::env::var_os(UNIT_VAR).filter(|unit| !unit.is_empty())
}

/// Whether the session starts the daemon's unit.
fn starts_with_session() -> bool {
    std::env::var_os(WITH_SESSION_VAR).is_some_and(|value| value == "1")
}

/// Starts the daemon the session would have started, if there is one.
/// Blocks until Emacs is ready: home-manager's service is `Type=notify`.
pub fn start_for_session() -> Result<Start, String> {
    let Some(unit) = daemon_unit().filter(|_| starts_with_session()) else {
        return Ok(Start::Skipped("none starts with your session"));
    };
    let start = match systemctl_start(unit) {
        Ok(output) => output,
        Err(err) if err.kind() == io::ErrorKind::NotFound => {
            return Ok(Start::Skipped("systemctl isn't on PATH"));
        }
        Err(err) => return Err(format!("cannot run systemctl: {err}")),
    };
    if start.status.success() {
        Ok(Start::Started)
    } else {
        Err(format!(
            "systemctl start failed ({}): {}",
            start.status,
            String::from_utf8_lossy(&start.stderr).trim()
        ))
    }
}

fn systemctl_start(unit: OsString) -> io::Result<std::process::Output> {
    Command::new("systemctl")
        .args(["--user", "start"])
        .arg(unit)
        .stdin(Stdio::null())
        .output()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restarts_through_systemd_only_the_sessions_daemon() {
        let unit = OsStr::new("emacs.service");
        assert_eq!(
            restart_advice(true, Some(unit)),
            "restart Emacs: systemctl --user restart emacs.service"
        );
        assert_eq!(restart_advice(true, None), "restart the Emacs daemon");
        // A standalone Emacs holding the socket isn't the session's unit.
        assert_eq!(restart_advice(false, Some(unit)), "restart Emacs");
        assert_eq!(restart_advice(false, None), "restart Emacs");
    }
}
