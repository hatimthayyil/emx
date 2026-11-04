//! Runs Nix through its command line, the interface Nix keeps stable.

use std::ffi::{OsStr, OsString};
use std::io;
use std::path::Path;
use std::process::Command;

use crate::error::Error;

/// Arguments for `nix build` that build `installable` into a new generation
/// of `profile`, followed by the caller's `extra` arguments. The build uses the
/// flake's lock as it is and fails if the lock is out of date.
pub fn build_into_profile(
    installable: &OsStr,
    profile: &Path,
    extra: &[OsString],
) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "build",
        "--extra-experimental-features",
        "nix-command flakes",
        "--no-link",
        "--no-update-lock-file",
        "--profile",
    ]
    .map(OsString::from)
    .into();
    args.push(profile.into());
    args.push(installable.into());
    args.extend(extra.iter().cloned());
    args
}

/// Runs `nix` with `args`, its output going straight to the terminal.
pub fn run(args: &[OsString]) -> Result<(), Error> {
    let status = Command::new("nix")
        .args(args)
        .status()
        .map_err(|source| match source.kind() {
            io::ErrorKind::NotFound => Error::NixMissing,
            _ => Error::NixSpawn { source },
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::NixFailed { status })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_into_the_profile_from_the_locked_flake() {
        let args = build_into_profile(
            OsStr::new("/emx#emacs"),
            Path::new("/state/emx/profile"),
            &["--max-jobs".into(), "2".into()],
        );
        assert_eq!(
            args,
            [
                "build",
                "--extra-experimental-features",
                "nix-command flakes",
                "--no-link",
                "--no-update-lock-file",
                "--profile",
                "/state/emx/profile",
                "/emx#emacs",
                "--max-jobs",
                "2",
            ]
        );
    }
}
