//! Runs Nix through its command line, the interface Nix keeps stable.

use std::collections::{BTreeSet, HashMap};
use std::ffi::{OsStr, OsString};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;

use crate::error::Error;

/// How a build reports its progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// Nix's own output, straight to the terminal.
    Plain,
    /// Nix's structured log, rendered by nix-output-monitor.
    Nom,
}

/// Arguments for `nix build` that build `installable` and link the result at
/// `out_link`, a garbage collector root, followed by the caller's `extra`
/// arguments. The build uses the flake's lock as it is and fails if the lock
/// is out of date.
pub fn build(
    installable: &OsStr,
    out_link: &Path,
    progress: Progress,
    extra: &[OsString],
) -> Vec<OsString> {
    let mut args = base(&["build", "--no-update-lock-file", "--out-link"]);
    args.push(out_link.into());
    args.push(installable.into());
    if progress == Progress::Nom {
        args.extend(["--log-format", "internal-json", "--verbose"].map(OsString::from));
    }
    args.extend(extra.iter().cloned());
    args
}

/// Arguments for `nix build` that make `store_path`, already built, a new
/// generation of `profile`.
pub fn set_profile(profile: &Path, store_path: &Path) -> Vec<OsString> {
    let mut args = base(&["build", "--no-link", "--profile"]);
    args.push(profile.into());
    args.push(store_path.into());
    args
}

/// Arguments for `nix flake update` that move `inputs` of the flake at `flake`,
/// or every input when `inputs` is empty, followed by the caller's `extra`
/// arguments.
pub fn update_inputs(flake: &Path, inputs: &[String], extra: &[OsString]) -> Vec<OsString> {
    let mut args = base(&["flake", "update", "--flake"]);
    args.push(flake.into());
    args.extend(inputs.iter().map(OsString::from));
    args.extend(extra.iter().cloned());
    args
}

/// Arguments for `nix run` on `installable`, an app that reads the package
/// archives live and so needs `--impure`. The caller's `extra` arguments come
/// first: Nix hands the app whatever follows the installable.
pub fn run_app(installable: &OsStr, extra: &[OsString]) -> Vec<OsString> {
    let mut args = base(&["run", "--impure"]);
    args.extend(extra.iter().cloned());
    args.push(installable.into());
    args
}

/// Arguments for `nix profile rollback` that return `profile` to its previous
/// generation.
pub fn rollback(profile: &Path) -> Vec<OsString> {
    let mut args = base(&["profile", "rollback", "--profile"]);
    args.push(profile.into());
    args
}

/// Arguments for `nix derivation show` on the derivations that built `paths`.
pub fn show_derivations(paths: &[&Path]) -> Vec<OsString> {
    let mut args = base(&["derivation", "show"]);
    args.extend(paths.iter().map(|path| path.as_os_str().to_owned()));
    args
}

fn base(command: &[&str]) -> Vec<OsString> {
    let mut args: Vec<OsString> = command.iter().map(OsString::from).collect();
    args.splice(
        1..1,
        ["--extra-experimental-features", "nix-command flakes"].map(OsString::from),
    );
    args
}

/// Runs `nix` with `args`, its output going straight to the terminal.
pub fn run(args: &[OsString]) -> Result<(), Error> {
    let status = nix().args(args).status().map_err(spawn_error)?;
    check(status)
}

/// Runs `nix` with `args` in `dir`, passing its log on to the terminal, and
/// returns each location it could not fetch. Nix draws no progress bar on the
/// pipe this reads its log from.
pub fn run_noting_failed_fetches(dir: &Path, args: &[OsString]) -> Result<BTreeSet<String>, Error> {
    let mut child = nix()
        .args(args)
        .current_dir(dir)
        .stderr(Stdio::piped())
        .spawn()
        .map_err(spawn_error)?;
    let mut log = BufReader::new(child.stderr.take().expect("nix's stderr is piped"));
    let mut failed = BTreeSet::new();
    let mut line = Vec::new();
    while log.read_until(b'\n', &mut line).map_err(spawn_error)? > 0 {
        // Losing the terminal must not stop the log being read.
        let _ = io::stderr().write_all(&line);
        if let Some(location) = failed_fetch(&String::from_utf8_lossy(&line)) {
            failed.insert(location.to_owned());
        }
        line.clear();
    }
    check(child.wait().map_err(spawn_error)?)?;
    Ok(failed)
}

/// The location in a line of Nix's log that says a fetch failed and Nix went
/// on with what it had cached. Nix then exits 0 and leaves the pin where it
/// was. No Nix option makes this fatal and `--log-format internal-json` carries
/// the same text, so this reads Nix's wording: the test
/// `names_what_the_real_nix_could_not_fetch` holds it to the installed Nix.
fn failed_fetch(line: &str) -> Option<&str> {
    const FELL_BACK: [&str; 2] = [
        "; continuing with the most recent version",
        "; using cached version",
    ];
    FELL_BACK
        .iter()
        .any(|ending| line.contains(ending))
        .then(|| line.split('\'').nth(1).unwrap_or(line.trim()))
}

/// Runs `nix` with `args`, its log rendered by `nom --json`. Only Nix's exit
/// status counts: nom succeeds even when the build fails.
pub fn run_with_nom(args: &[OsString]) -> Result<(), Error> {
    let mut nom = Command::new("nom")
        .arg("--json")
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|source| match source.kind() {
            io::ErrorKind::NotFound => Error::NomMissing,
            _ => Error::NomSpawn { source },
        })?;
    let log = nom.stdin.take().expect("nom's stdin is piped");
    // Nix writes its log to stderr; dropping the command closes the pipe, so
    // nom sees the end of the log once Nix exits.
    let status = nix().args(args).stderr(log).status();
    // Its exit status says nothing about the build.
    let _ = nom.wait();
    check(status.map_err(spawn_error)?)
}

/// Runs `nix` with `args` and returns what it prints, or a message when it
/// fails.
pub fn output(args: &[OsString]) -> Result<Vec<u8>, String> {
    let output = nix()
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|err| spawn_error(err).to_string())?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(format!(
            "nix {} failed ({}): {}",
            args.first()
                .map(|a| a.to_string_lossy())
                .unwrap_or_default(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn nix() -> Command {
    Command::new("nix")
}

fn spawn_error(source: io::Error) -> Error {
    match source.kind() {
        io::ErrorKind::NotFound => Error::NixMissing,
        _ => Error::NixSpawn { source },
    }
}

fn check(status: std::process::ExitStatus) -> Result<(), Error> {
    if status.success() {
        Ok(())
    } else {
        Err(Error::NixFailed { status })
    }
}

/// Each output's store file name, `<hash>-<name>`, mapped to the `src` of the
/// derivation that built it, from `nix derivation show`'s JSON. Nix 2.33 and
/// later nest the derivations under `derivations` and name outputs without
/// `/nix/store/`.
pub fn derivation_sources(json: &[u8]) -> Result<HashMap<String, PathBuf>, serde_json::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Document {
        Nested {
            derivations: HashMap<String, Derivation>,
        },
        Flat(HashMap<String, Derivation>),
    }
    #[derive(Deserialize)]
    struct Derivation {
        outputs: HashMap<String, Output>,
        env: HashMap<String, String>,
    }
    #[derive(Deserialize)]
    struct Output {
        path: Option<PathBuf>,
    }

    let derivations = match serde_json::from_slice(json)? {
        Document::Nested { derivations } | Document::Flat(derivations) => derivations,
    };
    let mut sources = HashMap::new();
    for derivation in derivations.into_values() {
        let Some(src) = derivation.env.get("src") else {
            continue;
        };
        for output in derivation.outputs.into_values() {
            let name = output
                .path
                .as_deref()
                .and_then(Path::file_name)
                .and_then(OsStr::to_str);
            if let Some(name) = name {
                sources.insert(name.to_owned(), PathBuf::from(src));
            }
        }
    }
    Ok(sources)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_location_nix_failed_to_fetch_from_git() {
        assert_eq!(
            failed_fetch(
                "warning: could not update local clone of Git repository \
                 'https://example.org/repo.git'; continuing with the most recent version\n"
            ),
            Some("https://example.org/repo.git")
        );
    }

    #[test]
    fn finds_the_location_nix_failed_to_download() {
        assert_eq!(
            failed_fetch(
                "warning: unable to download \
                 'https://api.github.com/repos/minad/vertico/commits/HEAD': \
                 HTTP error 500; using cached version\n"
            ),
            Some("https://api.github.com/repos/minad/vertico/commits/HEAD")
        );
    }

    #[test]
    fn keeps_a_failed_fetch_that_names_no_location() {
        assert_eq!(
            failed_fetch("warning: no network; using cached version\n"),
            Some("warning: no network; using cached version")
        );
    }

    #[test]
    fn ignores_warnings_that_leave_a_refresh_intact() {
        for line in [
            "warning: could not update mtime for file '/cache/ref': Permission denied",
            "warning: could not get HEAD ref for repository 'https://example.org/repo.git'; \
             using expired cached ref 'refs/heads/main'",
            "warning: updating lock file '/emx/lock/flake.lock':",
        ] {
            assert_eq!(failed_fetch(line), None, "{line}");
        }
    }

    #[test]
    fn builds_into_a_root_from_the_locked_flake() {
        let args = build(
            OsStr::new("/emx#emacs"),
            Path::new("/state/emx/pending"),
            Progress::Plain,
            &["--max-jobs".into(), "2".into()],
        );
        assert_eq!(
            args,
            [
                "build",
                "--extra-experimental-features",
                "nix-command flakes",
                "--no-update-lock-file",
                "--out-link",
                "/state/emx/pending",
                "/emx#emacs",
                "--max-jobs",
                "2",
            ]
        );
    }

    #[test]
    fn asks_for_a_structured_log_for_nom() {
        let args = build(
            OsStr::new("/emx#emacs"),
            Path::new("/state/emx/pending"),
            Progress::Nom,
            &[],
        );
        assert_eq!(
            args[args.len() - 3..],
            ["--log-format", "internal-json", "--verbose"]
        );
    }

    #[test]
    fn sets_a_profile_generation_to_a_built_path() {
        assert_eq!(
            set_profile(
                Path::new("/state/emx/profile"),
                Path::new("/nix/store/abc-emacs")
            ),
            [
                "build",
                "--extra-experimental-features",
                "nix-command flakes",
                "--no-link",
                "--profile",
                "/state/emx/profile",
                "/nix/store/abc-emacs",
            ]
        );
    }

    #[test]
    fn moves_named_inputs_of_a_flake() {
        assert_eq!(
            update_inputs(
                Path::new("/emx/lock"),
                &["gptel".to_owned()],
                &["--cores".into(), "4".into()]
            ),
            [
                "flake",
                "--extra-experimental-features",
                "nix-command flakes",
                "update",
                "--flake",
                "/emx/lock",
                "gptel",
                "--cores",
                "4",
            ]
        );
    }

    #[test]
    fn keeps_arguments_for_nix_ahead_of_the_app() {
        let args = run_app(OsStr::new("/emx#lock"), &["--cores".into(), "4".into()]);
        assert_eq!(
            args[args.len() - 4..],
            ["--impure", "--cores", "4", "/emx#lock"]
        );
    }

    #[test]
    fn rolls_a_profile_back() {
        assert_eq!(
            rollback(Path::new("/state/emx/profile")),
            [
                "profile",
                "--extra-experimental-features",
                "nix-command flakes",
                "rollback",
                "--profile",
                "/state/emx/profile",
            ]
        );
    }

    #[test]
    fn reads_sources_from_both_derivation_formats() {
        let nested = br#"{"version": 4, "derivations": {"d-gptel.drv": {
            "outputs": {"out": {"path": "h-gptel-1.0"}},
            "env": {"src": "/nix/store/s-source"}}}}"#;
        let flat = br#"{"/nix/store/d-gptel.drv": {
            "outputs": {"out": {"path": "/nix/store/h-gptel-1.0"}},
            "env": {"src": "/nix/store/s-source"}}}"#;
        for json in [&nested[..], &flat[..]] {
            assert_eq!(
                derivation_sources(json).unwrap(),
                HashMap::from([("h-gptel-1.0".to_owned(), "/nix/store/s-source".into())])
            );
        }
    }
}
