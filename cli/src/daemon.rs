//! Starts home-manager's Emacs daemon after the sync that creates emx's
//! profile. `emacs.service` skips while the profile is missing
//! (nix/home-module.nix), so a switch before that sync leaves it stopped.

use std::io;
use std::process::{Command, Output, Stdio};

const SERVICE: &str = "emacs.service";

/// Starts the daemon when its service is enabled, and says whether it did.
/// A socket-activated service reads as `static`, and its socket starts it.
/// Blocks until Emacs is ready: the service is `Type=notify`.
pub fn start_if_enabled() -> Result<bool, String> {
    let state = match systemctl("is-enabled") {
        Ok(output) => output,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(format!("cannot run systemctl: {err}")),
    };
    if state.stdout.trim_ascii() != b"enabled" {
        return Ok(false);
    }
    let start = systemctl("start").map_err(|err| format!("cannot run systemctl: {err}"))?;
    if start.status.success() {
        Ok(true)
    } else {
        Err(format!(
            "systemctl start failed ({}): {}",
            start.status,
            String::from_utf8_lossy(&start.stderr).trim()
        ))
    }
}

fn systemctl(verb: &str) -> io::Result<Output> {
    Command::new("systemctl")
        .args(["--user", verb, SERVICE])
        .stdin(Stdio::null())
        .output()
}
