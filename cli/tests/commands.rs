//! emx's commands run as a program. A fake `nix` on PATH records each call's
//! arguments and does what the real one would to the build root and the
//! profile, and a fake `systemctl` records its calls and answers as told, so
//! these tests need neither Nix nor the user's state directory or daemon. An
//! Emacs server is a socket the test listens on in its own runtime directory,
//! and a generation's fake `emacsclient` answers its reload as told.
//! Their stdin is not a terminal, so emx never asks and needs `--yes`.

use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

const FAKE_NIX: &str = r#"#!/bin/sh
printf '%s\n' "$@" -- >> "$FAKE_NIX_ARGS"
pwd >> "$FAKE_NIX_DIRS"
[ "$1" != flake ] || [ -z "$REAL_NIX" ] || exec "$REAL_NIX" "$@"
[ "$1" != flake ] || [ -z "$FAKE_NIX_UPDATE_LOG" ] || echo "$FAKE_NIX_UPDATE_LOG" >&2
[ "$1" != run ] || [ -z "$FAKE_NIX_RUN_LOG" ] || echo "$FAKE_NIX_RUN_LOG" >&2
[ "${FAKE_NIX_STATUS:-0}" = 0 ] || exit "$FAKE_NIX_STATUS"
[ "$1" != build ] || [ "${FAKE_NIX_BUILD_STATUS:-0}" = 0 ] || exit "$FAKE_NIX_BUILD_STATUS"
link= profile= target= rollback=
while [ $# -gt 0 ]; do
  case "$1" in
    --out-link) link=$2; shift ;;
    --profile) profile=$2; shift ;;
    rollback) rollback=1 ;;
    /*) target=$1 ;;
  esac
  shift
done
if [ -n "$link" ]; then
  mkdir -p "$FAKE_NIX_OUT"
  ln -sfn "$FAKE_NIX_OUT" "$link"
fi
if [ -n "$rollback" ]; then
  n=$(readlink "$profile")
  n=${n#profile-}
  n=${n%-link}
  [ "$n" -gt 1 ] || { echo "no older generation" >&2; exit 1; }
  ln -sfn "$(basename "$profile")-$((n - 1))-link" "$profile"
elif [ -n "$profile" ]; then
  n=1
  while [ -L "$profile-$n-link" ]; do n=$((n + 1)); done
  ln -s "$target" "$profile-$n-link"
  ln -sfn "$(basename "$profile")-$n-link" "$profile"
fi
"#;

/// Exits as the real one does: `is-enabled` fails for a unit that is missing.
const FAKE_SYSTEMCTL: &str = r#"#!/bin/sh
printf '%s\n' "$*" >> "$FAKE_SYSTEMCTL_ARGS"
case "$2" in
  is-enabled)
    echo "$FAKE_SYSTEMCTL_STATE"
    [ "$FAKE_SYSTEMCTL_STATE" != not-found ] || exit 4
    ;;
  start)
    if [ -n "$FAKE_SYSTEMCTL_START_ERROR" ]; then
      echo "$FAKE_SYSTEMCTL_START_ERROR" >&2
      exit 1
    fi
    ;;
esac
"#;

const IS_ENABLED: &str = "--user is-enabled emacs.service";
const START: &str = "--user start emacs.service";

/// Records the file it runs as and its arguments, then writes the report that
/// `emx-reload-write-report` would to the file the Elisp names, with the
/// outcome in FAKE_EMACSCLIENT and, if FAKE_EMACSCLIENT_DAEMON is `true`, from
/// a daemon. `fail` fails as emacsclient does when Emacs signals an error, and
/// `garbage` writes a report that isn't JSON.
const FAKE_EMACSCLIENT: &str = r#"#!/bin/sh
printf '%s\n' "$(readlink -f "$0")" "$@" -- >> "$FAKE_EMACSCLIENT_ARGS"
expression=
while [ $# -gt 0 ]; do
  [ "$1" != --eval ] || expression=$2
  shift
done
report=$(printf '%s' "$expression" | sed -n 's/.*(emx-reload-write-report "\([^"]*\)".*/\1/p')
case "$FAKE_EMACSCLIENT" in
  fail)
    echo "*ERROR*: Wrong type argument: stringp, nil" >&2
    exit 1
    ;;
  garbage) echo 'not a report' > "$report" ;;
  *) printf '{"outcome": "%s", "report": "emx-reload: a report for %s", "daemon": %s}' \
       "$FAKE_EMACSCLIENT" "$FAKE_EMACSCLIENT" "${FAKE_EMACSCLIENT_DAEMON:-false}" > "$report" ;;
esac
"#;

const RESTART: &str = "systemctl --user restart emacs";

fn write_executable(path: &Path, script: &str) {
    fs::write(path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// This process's PATH with `dir` in front.
fn path_with(dir: &Path) -> OsString {
    std::env::join_paths(std::iter::once(dir.to_owned()).chain(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    )))
    .unwrap()
}

struct Fixture {
    root: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let src = root.path().join("emx");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("flake.nix"), "").unwrap();
        fs::write(src.join("packages.el"), "").unwrap();
        fs::create_dir(src.join("lock")).unwrap();
        fs::write(src.join("lock/archive.lock"), r#"{"denote": {}}"#).unwrap();
        fs::write(
            src.join("lock/flake.lock"),
            r#"{"nodes": {
                "root": {"inputs": {"gptel": "gptel", "magit": "magit"}},
                "gptel": {"original": {"type": "github", "owner": "karthink", "repo": "gptel"}},
                "magit": {"original": {"type": "git", "url": "https://example.org/magit.git"}}
            }}"#,
        )
        .unwrap();
        let bin = root.path().join("bin");
        fs::create_dir(&bin).unwrap();
        write_executable(&bin.join("nix"), FAKE_NIX);
        write_executable(&bin.join("systemctl"), FAKE_SYSTEMCTL);
        fs::create_dir(root.path().join("state")).unwrap();
        Self { root }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }

    fn state(&self, name: &str) -> PathBuf {
        self.path("state").join("emx").join(name)
    }

    /// `emx` with an environment that reaches only the fixture's directories,
    /// building `store-emacs`, with no `emacs.service` and no Emacs server.
    fn emx(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_emx"));
        command
            .env_remove("EMX_SRC")
            .env_remove("EMACS_SOCKET_NAME")
            .env("PATH", path_with(&self.path("bin")))
            .env("HOME", self.path("home"))
            .env("XDG_STATE_HOME", self.path("state"))
            .env("XDG_RUNTIME_DIR", self.path("runtime"))
            .env("FAKE_EMACSCLIENT_ARGS", self.path("emacsclient-args"))
            .env("FAKE_NIX_ARGS", self.path("nix-args"))
            .env("FAKE_NIX_DIRS", self.path("nix-dirs"))
            .env("FAKE_NIX_OUT", self.path("store-emacs"))
            .env("FAKE_SYSTEMCTL_ARGS", self.path("systemctl-args"))
            .env("FAKE_SYSTEMCTL_STATE", "not-found");
        command
    }

    fn sync(&self) -> Command {
        let mut command = self.emx();
        command.env("EMX_SRC", self.path("emx")).arg("sync");
        command
    }

    fn upgrade(&self) -> Command {
        let mut command = self.emx();
        command.env("EMX_SRC", self.path("emx")).arg("upgrade");
        command
    }

    fn rollback(&self) -> Command {
        let mut command = self.emx();
        command.arg("rollback");
        command
    }

    /// The arguments of each `nix` call, in order.
    fn nix_calls(&self) -> Vec<Vec<String>> {
        let Ok(text) = fs::read_to_string(self.path("nix-args")) else {
            return Vec::new();
        };
        text.split_terminator("--\n")
            .map(|call| call.lines().map(str::to_owned).collect())
            .collect()
    }

    /// The directory of each `nix` call, in order.
    fn nix_dirs(&self) -> Vec<PathBuf> {
        fs::read_to_string(self.path("nix-dirs"))
            .map(|text| text.lines().map(PathBuf::from).collect())
            .unwrap_or_default()
    }

    /// The socket emx finds an Emacs server on.
    fn socket(&self) -> PathBuf {
        self.path("runtime/emacs/server")
    }

    /// An Emacs server, for as long as the listener lives. Nothing accepts:
    /// emx only connects, and the fake `emacsclient` answers.
    fn serve(&self) -> UnixListener {
        fs::create_dir_all(self.path("runtime/emacs")).unwrap();
        UnixListener::bind(self.socket()).unwrap()
    }

    /// Gives the build at `store_path`, a fixture path, a fake `emacsclient`.
    fn give_emacsclient(&self, store_path: &str) {
        let bin = self.path(store_path).join("bin");
        fs::create_dir_all(&bin).unwrap();
        write_executable(&bin.join("emacsclient"), FAKE_EMACSCLIENT);
    }

    /// The file each `emacsclient` call ran as, then its arguments, in order.
    fn emacsclient_calls(&self) -> Vec<Vec<String>> {
        let Ok(text) = fs::read_to_string(self.path("emacsclient-args")) else {
            return Vec::new();
        };
        text.split_terminator("--\n")
            .map(|call| call.lines().map(str::to_owned).collect())
            .collect()
    }

    /// The arguments of each `systemctl` call, in order.
    fn systemctl_calls(&self) -> Vec<String> {
        fs::read_to_string(self.path("systemctl-args"))
            .map(|text| text.lines().map(str::to_owned).collect())
            .unwrap_or_default()
    }

    /// A PATH holding the fake `nix` and the tools it calls, and no
    /// `systemctl`. This process's PATH cannot be filtered instead: on NixOS
    /// one directory holds both coreutils and `systemctl`.
    fn path_without_systemctl(&self) -> PathBuf {
        let bin = self.path("bin-without-systemctl");
        fs::create_dir(&bin).unwrap();
        symlink(self.path("bin/nix"), bin.join("nix")).unwrap();
        let path = std::env::var_os("PATH").unwrap_or_default();
        for tool in ["basename", "ln", "mkdir"] {
            let found = std::env::split_paths(&path)
                .map(|dir| dir.join(tool))
                .find(|candidate| candidate.exists())
                .unwrap();
            symlink(found, bin.join(tool)).unwrap();
        }
        bin
    }
}

fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap()
}

fn assert_exit(output: &Output, code: i32) {
    assert_eq!(
        output.status.code(),
        Some(code),
        "stdout: {}\nstderr: {}",
        text(&output.stdout),
        text(&output.stderr)
    );
}

fn after<'a>(args: &'a [String], flag: &str) -> &'a str {
    let at = args.iter().position(|a| a == flag).unwrap();
    &args[at + 1]
}

#[test]
fn builds_into_a_root_then_switches_the_profile() {
    let fixture = Fixture::new();
    let output = fixture
        .sync()
        .args(["--yes", "--", "--max-jobs", "2"])
        .output()
        .unwrap();
    assert_exit(&output, 0);

    let calls = fixture.nix_calls();
    assert_eq!(calls.len(), 2, "{calls:?}");
    let (build, switch) = (&calls[0], &calls[1]);
    let src = fixture.path("emx").canonicalize().unwrap();
    assert_eq!(build[0], "build");
    assert!(build.contains(&"--no-update-lock-file".to_owned()));
    assert_eq!(
        Path::new(after(build, "--out-link")),
        fixture.state("pending")
    );
    assert!(build.contains(&format!("{}#emacs", src.display())));
    assert_eq!(build[build.len() - 2..], ["--max-jobs", "2"]);
    assert_eq!(
        Path::new(after(switch, "--profile")),
        fixture.state("profile")
    );
    assert_eq!(
        switch.last().map(PathBuf::from),
        Some(fixture.path("store-emacs"))
    );

    assert_eq!(
        text(&output.stdout),
        format!(
            "emx: {} is now generation 1: {}\n",
            fixture.state("profile").display(),
            fixture.path("store-emacs").display()
        )
    );
    // The fake build is no store path, so emx cannot diff it, says so, and
    // still switches.
    assert!(text(&output.stderr).contains("cannot show what the build changes"));
    assert!(!fixture.state("pending").exists());
}

#[test]
fn switches_to_a_changed_build_as_a_new_generation() {
    let fixture = Fixture::new();
    assert_exit(&fixture.sync().arg("--yes").output().unwrap(), 0);
    let output = fixture
        .sync()
        .arg("--yes")
        .env("FAKE_NIX_OUT", fixture.path("store-emacs-2"))
        .output()
        .unwrap();
    assert_exit(&output, 0);
    assert!(text(&output.stdout).contains("is now generation 2"));
    assert_eq!(
        fs::read_link(fixture.state("profile")).unwrap(),
        Path::new("profile-2-link")
    );
}

#[test]
fn keeps_the_profile_when_nothing_changed() {
    let fixture = Fixture::new();
    assert_exit(&fixture.sync().arg("--yes").output().unwrap(), 0);
    let output = fixture.sync().arg("--yes").output().unwrap();
    assert_exit(&output, 0);
    assert_eq!(
        text(&output.stdout),
        format!(
            "emx: nothing changed: generation 1 is already {}\n",
            fixture.path("store-emacs").display()
        )
    );
    // Two calls for the first sync, then only the build.
    assert_eq!(fixture.nix_calls().len(), 3);
    assert!(!fixture.state("profile-2-link").exists());
    assert!(!fixture.state("pending").exists());
}

#[test]
fn starts_an_enabled_daemon_after_the_sync_that_creates_the_profile() {
    let fixture = Fixture::new();
    let output = fixture
        .sync()
        .arg("--yes")
        .env("FAKE_SYSTEMCTL_STATE", "enabled")
        .output()
        .unwrap();
    assert_exit(&output, 0);
    assert_eq!(fixture.systemctl_calls(), [IS_ENABLED, START]);
    assert!(text(&output.stdout).ends_with("emx: started the Emacs daemon\n"));
}

#[test]
fn starts_no_daemon_unless_its_service_is_enabled() {
    for state in ["static", "disabled", "not-found"] {
        let fixture = Fixture::new();
        let output = fixture
            .sync()
            .arg("--yes")
            .env("FAKE_SYSTEMCTL_STATE", state)
            .output()
            .unwrap();
        assert_exit(&output, 0);
        assert_eq!(fixture.systemctl_calls(), [IS_ENABLED], "{state}");
        assert!(!text(&output.stdout).contains("daemon"), "{state}");
        assert!(!text(&output.stderr).contains("daemon"), "{state}");
    }
}

#[test]
fn says_nothing_about_the_daemon_without_systemctl() {
    let fixture = Fixture::new();
    let output = fixture
        .sync()
        .arg("--yes")
        .env("PATH", fixture.path_without_systemctl())
        .output()
        .unwrap();
    assert_exit(&output, 0);
    assert!(text(&output.stdout).contains("is now generation 1"));
    assert!(!text(&output.stdout).contains("daemon"));
    assert!(!text(&output.stderr).contains("daemon"));
}

#[test]
fn leaves_the_daemon_alone_after_later_syncs() {
    let fixture = Fixture::new();
    let first = fixture
        .sync()
        .arg("--yes")
        .env("FAKE_SYSTEMCTL_STATE", "enabled")
        .output()
        .unwrap();
    assert_exit(&first, 0);
    let second = fixture
        .sync()
        .arg("--yes")
        .env("FAKE_SYSTEMCTL_STATE", "enabled")
        .env("FAKE_NIX_OUT", fixture.path("store-emacs-2"))
        .output()
        .unwrap();
    assert_exit(&second, 0);
    assert!(text(&second.stdout).contains("is now generation 2"));
    assert_eq!(fixture.systemctl_calls(), [IS_ENABLED, START]);
}

#[test]
fn warns_when_the_daemon_fails_to_start_and_keeps_the_switch() {
    let fixture = Fixture::new();
    let output = fixture
        .sync()
        .arg("--yes")
        .env("FAKE_SYSTEMCTL_STATE", "enabled")
        .env("FAKE_SYSTEMCTL_START_ERROR", "Job for emacs.service failed")
        .output()
        .unwrap();
    assert_exit(&output, 0);
    assert_eq!(fixture.systemctl_calls(), [IS_ENABLED, START]);
    assert!(text(&output.stdout).contains("is now generation 1"));
    assert!(!text(&output.stdout).contains("daemon"));
    let stderr = text(&output.stderr);
    assert!(stderr.contains("cannot start the Emacs daemon"), "{stderr}");
    assert!(stderr.contains("Job for emacs.service failed"), "{stderr}");
}

/// A profile whose current generation is `store-old`, switched to `store-emacs`
/// by `emx sync` while an Emacs server runs whose reload ends in `outcome`.
fn sync_with_a_server(outcome: &str) -> (Fixture, Output) {
    sync_with_a_server_of(outcome, false)
}

/// As `sync_with_a_server`, with the server a daemon if `daemon`.
fn sync_with_a_server_of(outcome: &str, daemon: bool) -> (Fixture, Output) {
    let fixture = synced_fixture();
    fixture.give_emacsclient("store-emacs");
    let _server = fixture.serve();
    let output = fixture
        .sync()
        .arg("--yes")
        .env("FAKE_EMACSCLIENT", outcome)
        .env("FAKE_EMACSCLIENT_DAEMON", daemon.to_string())
        .output()
        .unwrap();
    (fixture, output)
}

#[test]
fn reloads_a_running_emacs_after_switching() {
    let (fixture, output) = sync_with_a_server("reloaded");
    assert_exit(&output, 0);
    let calls = fixture.emacsclient_calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    let call = &calls[0];
    // The new generation's emacsclient, at the server's socket.
    assert_eq!(
        Path::new(&call[0]),
        fixture
            .path("store-emacs/bin/emacsclient")
            .canonicalize()
            .unwrap()
    );
    assert_eq!(
        call[1],
        format!("--socket-name={}", fixture.socket().display())
    );
    assert_eq!(call[2], "--eval");
    // Into the manifest of the profile's new generation, so Emacs needn't
    // locate the profile itself.
    assert!(
        call[3].contains(&format!(
            "(emx-reload-write-report \"{}\" \"{}\")",
            fixture.state("reload.json").display(),
            fixture.state("profile/share/emx/manifest.json").display()
        )),
        "{call:?}"
    );
    let stdout = text(&output.stdout);
    assert!(
        stdout.ends_with("emx-reload: a report for reloaded\n"),
        "{stdout}"
    );
    assert!(!stdout.contains("restart"), "{stdout}");
    assert!(!fixture.state("reload.json").exists());
}

#[test]
fn asks_for_a_restart_when_the_reload_cannot_load_everything() {
    for outcome in ["restart-recommended", "restart-required"] {
        // Only a daemon restarts through systemd.
        for (daemon, how) in [(true, format!(": {RESTART}")), (false, String::new())] {
            let (_fixture, output) = sync_with_a_server_of(outcome, daemon);
            assert_exit(&output, 0);
            let stdout = text(&output.stdout);
            assert!(
                stdout.ends_with(&format!(
                    "emx-reload: a report for {outcome}\n\
                     emx: restart Emacs{how}\n"
                )),
                "{outcome}, daemon {daemon}: {stdout}"
            );
        }
    }
}

#[test]
fn reloads_nothing_in_an_emacs_without_emx() {
    let (fixture, output) = sync_with_a_server("not-emx");
    assert_exit(&output, 0);
    let stdout = text(&output.stdout);
    assert!(
        stdout.ends_with("emx-reload: a report for not-emx\n"),
        "{stdout}"
    );
    assert!(!stdout.contains("restart"), "{stdout}");
    // An Emacs without emx's configuration has no emx-source-dir, so the
    // expression checks for it before loading emx-reload.
    let calls = fixture.emacsclient_calls();
    assert!(
        calls[0][3].starts_with("(if (boundp 'emx-source-dir) "),
        "{calls:?}"
    );
}

#[test]
fn fails_keeping_the_switch_when_the_reload_fails() {
    for (outcome, said) in [
        (
            "fail",
            "*ERROR*: Wrong type argument: stringp, nil\n\
             emx: emx's profile is switched, but the running Emacs didn't reload: \
             emacsclient failed (exit status: 1); its output is above",
        ),
        ("garbage", "cannot parse its report"),
    ] {
        let (fixture, output) = sync_with_a_server(outcome);
        assert_exit(&output, 1);
        assert_eq!(
            fs::read_link(fixture.state("profile")).unwrap(),
            Path::new("profile-2-link"),
            "{outcome}"
        );
        assert!(text(&output.stdout).contains("is now generation 2"));
        let stderr = text(&output.stderr);
        assert!(
            stderr.contains("emx's profile is switched, but the running Emacs didn't reload"),
            "{outcome}: {stderr}"
        );
        assert!(stderr.contains(said), "{outcome}: {stderr}");
        // With no report, emx can't tell a daemon from a standalone Emacs.
        assert!(
            stderr.contains("Restart Emacs to load the new generation"),
            "{outcome}: {stderr}"
        );
        assert!(!stderr.contains(RESTART), "{outcome}: {stderr}");
        assert!(!fixture.state("reload.json").exists(), "{outcome}");
    }
}

#[test]
fn reloads_nothing_without_a_running_emacs() {
    for stale in [false, true] {
        let fixture = synced_fixture();
        fixture.give_emacsclient("store-emacs");
        if stale {
            // The socket file of an Emacs that died.
            drop(fixture.serve());
            assert!(fixture.socket().exists());
        }
        let output = fixture.sync().arg("--yes").output().unwrap();
        assert_exit(&output, 0);
        assert!(fixture.emacsclient_calls().is_empty(), "stale: {stale}");
        assert!(
            text(&output.stdout).ends_with(&format!(
                "is now generation 2: {}\n",
                fixture.path("store-emacs").display()
            )),
            "stale: {stale}"
        );
        assert!(
            !text(&output.stderr).contains("didn't reload"),
            "stale: {stale}"
        );
    }
}

#[test]
fn needs_yes_without_a_terminal_to_ask() {
    let fixture = Fixture::new();
    let output = fixture.sync().output().unwrap();
    assert_exit(&output, 1);
    assert!(text(&output.stderr).contains("pass --yes"));
    assert!(fixture.nix_calls().is_empty());
}

#[test]
fn takes_the_src_from_the_command_line() {
    let fixture = Fixture::new();
    let output = fixture
        .emx()
        .arg("--src")
        .arg(fixture.path("emx"))
        .args(["sync", "--yes"])
        .output()
        .unwrap();
    assert_exit(&output, 0);
}

#[test]
fn fails_without_a_src() {
    let fixture = Fixture::new();
    let output = fixture.emx().args(["sync", "--yes"]).output().unwrap();
    assert_exit(&output, 1);
    assert!(text(&output.stderr).contains("EMX_SRC"));
    assert!(fixture.nix_calls().is_empty());
}

#[test]
fn fails_when_the_src_is_not_emx() {
    let fixture = Fixture::new();
    let output = fixture
        .emx()
        .env("EMX_SRC", fixture.path("state"))
        .args(["sync", "--yes"])
        .output()
        .unwrap();
    assert_exit(&output, 1);
    assert!(text(&output.stderr).contains("is not an emx repo"));
}

#[test]
fn reports_a_failed_build() {
    let fixture = Fixture::new();
    let output = fixture
        .sync()
        .arg("--yes")
        .env("FAKE_NIX_STATUS", "3")
        .output()
        .unwrap();
    assert_exit(&output, 1);
    assert!(text(&output.stderr).contains("nix failed"));
    assert_eq!(fixture.nix_calls().len(), 1);
    assert!(!fixture.state("profile").exists());
}

#[test]
fn reports_missing_nix() {
    let fixture = Fixture::new();
    let output = fixture
        .sync()
        .arg("--yes")
        .env("PATH", fixture.path("empty"))
        .output()
        .unwrap();
    assert_exit(&output, 1);
    assert!(text(&output.stderr).contains("not on PATH"));
}

#[test]
fn rejects_a_missing_subcommand() {
    let fixture = Fixture::new();
    let output = fixture.emx().output().unwrap();
    assert_exit(&output, 2);
}

/// The verb of each `nix` call: `update` for `flake update`, `run`, `build`.
fn nix_verbs(fixture: &Fixture) -> Vec<String> {
    fixture
        .nix_calls()
        .iter()
        .map(|call| {
            let verb = call.iter().find(|arg| {
                matches!(
                    arg.as_str(),
                    "update" | "run" | "build" | "rollback" | "show"
                )
            });
            verb.unwrap().clone()
        })
        .collect()
}

/// What the real Nix logs when it can't refresh `magit`, a Git input.
const GIT_FETCH_FAILED: &str = "warning: could not update local clone of Git repository \
                                'https://example.org/magit.git'; \
                                continuing with the most recent version";

/// What the real Nix logs when it can't refresh `gptel`, a GitHub input.
const GITHUB_FETCH_FAILED: &str = "warning: unable to download \
                                   'https://api.github.com/repos/karthink/gptel/commits/HEAD': \
                                   HTTP error 500; using cached version";

/// What the real Nix logs when a package archive can't be read.
const ARCHIVE_FETCH_FAILED: &str = "warning: unable to download \
                                    'https://elpa.gnu.org/packages/archive-contents': \
                                    HTTP error 500; using cached version";

/// A profile whose current generation is `store-old`, as after a first sync.
fn synced_fixture() -> Fixture {
    let fixture = Fixture::new();
    let output = fixture
        .sync()
        .arg("--yes")
        .env("FAKE_NIX_OUT", fixture.path("store-old"))
        .output()
        .unwrap();
    assert_exit(&output, 0);
    fs::remove_file(fixture.path("nix-args")).unwrap();
    fs::remove_file(fixture.path("nix-dirs")).unwrap();
    fixture
}

#[test]
fn upgrades_every_package_with_the_update_app() {
    let fixture = synced_fixture();
    let output = fixture.upgrade().arg("--yes").output().unwrap();
    assert_exit(&output, 0);

    let calls = fixture.nix_calls();
    let src = fixture.path("emx").canonicalize().unwrap();
    assert_eq!(
        after(&calls[0], "--flake"),
        src.join("lock").to_str().unwrap()
    );
    assert_eq!(calls[0].last().unwrap(), src.join("lock").to_str().unwrap());
    assert!(calls[1].contains(&format!("{}#update", src.display())));
    assert!(calls[1].contains(&"--impure".to_owned()));
    assert!(calls[2].contains(&format!("{}#emacs", src.display())));
    assert_eq!(nix_verbs(&fixture)[..3], ["update", "run", "build"]);
    assert_eq!(fixture.nix_dirs()[1].canonicalize().unwrap(), src);
}

#[test]
fn upgrades_named_packages_with_the_lock_app() {
    let fixture = synced_fixture();
    let output = fixture
        .upgrade()
        .args(["gptel", "magit", "--yes"])
        .output()
        .unwrap();
    assert_exit(&output, 0);

    let calls = fixture.nix_calls();
    let src = fixture.path("emx").canonicalize().unwrap();
    assert_eq!(calls[0][calls[0].len() - 2..], ["gptel", "magit"]);
    assert!(calls[1].contains(&format!("{}#lock", src.display())));
    assert_eq!(fixture.nix_dirs()[1].canonicalize().unwrap(), src);
}

#[test]
fn passes_arguments_for_nix_to_each_step_that_fetches_or_builds() {
    let fixture = synced_fixture();
    let output = fixture
        .upgrade()
        .args(["gptel", "--yes", "--", "--cores", "4"])
        .output()
        .unwrap();
    assert_exit(&output, 0);

    for call in &fixture.nix_calls()[..3] {
        assert_eq!(after(call, "--cores"), "4", "{call:?}");
    }
}

#[test]
fn reloads_a_running_emacs_after_upgrading() {
    let fixture = synced_fixture();
    fixture.give_emacsclient("store-emacs");
    let _server = fixture.serve();
    let output = fixture
        .upgrade()
        .arg("--yes")
        .env("FAKE_EMACSCLIENT", "reloaded")
        .output()
        .unwrap();
    assert_exit(&output, 0);
    assert_eq!(
        fs::read_link(fixture.state("profile")).unwrap(),
        Path::new("profile-2-link")
    );
    assert_eq!(fixture.emacsclient_calls().len(), 1);
    let stdout = text(&output.stdout);
    assert!(
        stdout.ends_with("emx-reload: a report for reloaded\n"),
        "{stdout}"
    );
}

#[test]
fn reloads_nothing_when_nothing_changed() {
    let fixture = synced_fixture();
    fixture.give_emacsclient("store-old");
    let _server = fixture.serve();
    let output = fixture
        .upgrade()
        .arg("--yes")
        .env("FAKE_NIX_OUT", fixture.path("store-old"))
        .output()
        .unwrap();
    assert_exit(&output, 0);
    let stdout = text(&output.stdout);
    assert!(stdout.contains("nothing changed"), "{stdout}");
    assert!(!stdout.contains("restart"), "{stdout}");
    assert!(fixture.emacsclient_calls().is_empty());
}

#[test]
fn reloads_nothing_after_creating_the_profile() {
    let fixture = Fixture::new();
    fixture.give_emacsclient("store-emacs");
    let _server = fixture.serve();
    let output = fixture.upgrade().arg("--yes").output().unwrap();
    assert_exit(&output, 0);
    assert!(!text(&output.stdout).contains("restart"));
    assert!(fixture.emacsclient_calls().is_empty());
}

#[test]
fn refuses_an_elpa_package_before_touching_the_lock() {
    let fixture = Fixture::new();
    let output = fixture
        .upgrade()
        .args(["gptel", "denote", "--yes"])
        .output()
        .unwrap();
    assert_exit(&output, 1);
    let stderr = text(&output.stderr);
    assert!(stderr.contains("denote is an ELPA package"), "{stderr}");
    assert!(fixture.nix_calls().is_empty());
}

#[test]
fn refuses_an_unknown_package_before_touching_the_lock() {
    let fixture = Fixture::new();
    let output = fixture.upgrade().args(["gptl", "--yes"]).output().unwrap();
    assert_exit(&output, 1);
    let stderr = text(&output.stderr);
    assert!(stderr.contains("gptl is not locked"), "{stderr}");
    assert!(fixture.nix_calls().is_empty());
}

#[test]
fn upgrade_needs_yes_before_touching_the_lock() {
    let fixture = Fixture::new();
    let output = fixture.upgrade().output().unwrap();
    assert_exit(&output, 1);
    assert!(text(&output.stderr).contains("--yes"));
    assert!(fixture.nix_calls().is_empty());
}

#[test]
fn says_how_to_undo_the_pins_when_the_build_fails() {
    let fixture = synced_fixture();
    let output = fixture
        .upgrade()
        .arg("--yes")
        .env("FAKE_NIX_BUILD_STATUS", "3")
        .output()
        .unwrap();
    assert_exit(&output, 1);
    let stderr = text(&output.stderr);
    assert!(stderr.contains("`git restore lock`"), "{stderr}");
    assert!(stderr.contains("nix failed"), "{stderr}");
    assert!(!text(&output.stdout).contains("restart"));
}

#[test]
fn stops_when_the_lock_cannot_move() {
    let fixture = synced_fixture();
    let output = fixture
        .upgrade()
        .arg("--yes")
        .env("FAKE_NIX_STATUS", "3")
        .output()
        .unwrap();
    assert_exit(&output, 1);
    assert!(!text(&output.stderr).contains("git restore"));
    assert_eq!(fixture.nix_calls().len(), 1);
}

#[test]
fn finishes_the_upgrade_then_fails_naming_what_it_could_not_refresh() {
    let fixture = synced_fixture();
    let output = fixture
        .upgrade()
        .arg("--yes")
        .env(
            "FAKE_NIX_UPDATE_LOG",
            format!("{GIT_FETCH_FAILED}\n{GITHUB_FETCH_FAILED}"),
        )
        .env("FAKE_NIX_RUN_LOG", ARCHIVE_FETCH_FAILED)
        .output()
        .unwrap();
    assert_exit(&output, 1);

    assert_eq!(nix_verbs(&fixture)[..3], ["update", "run", "build"]);
    assert_eq!(
        fs::read_link(fixture.state("profile")).unwrap(),
        Path::new("profile-2-link")
    );
    let stderr = text(&output.stderr);
    for shown in [GIT_FETCH_FAILED, GITHUB_FETCH_FAILED, ARCHIVE_FETCH_FAILED] {
        assert!(stderr.contains(shown), "{stderr}");
    }
    assert!(
        stderr.contains(
            "could not refresh gptel, https://elpa.gnu.org/packages/archive-contents, magit: \
             pins stay as they were"
        ),
        "{stderr}"
    );
}

#[test]
fn names_what_it_could_not_refresh_when_the_build_fails_too() {
    let fixture = synced_fixture();
    let output = fixture
        .upgrade()
        .arg("--yes")
        .env("FAKE_NIX_UPDATE_LOG", GIT_FETCH_FAILED)
        .env("FAKE_NIX_BUILD_STATUS", "3")
        .output()
        .unwrap();
    assert_exit(&output, 1);
    let stderr = text(&output.stderr);
    assert!(stderr.contains("could not refresh magit"), "{stderr}");
    assert!(stderr.contains("nix failed"), "{stderr}");
    assert!(stderr.contains("`git restore lock`"), "{stderr}");
}

#[test]
fn rolls_back_to_the_previous_generation() {
    let fixture = synced_fixture();
    assert_exit(&fixture.upgrade().arg("--yes").output().unwrap(), 0);

    let output = fixture.rollback().output().unwrap();
    assert_exit(&output, 0);
    assert_eq!(
        fs::read_link(fixture.state("profile")).unwrap(),
        Path::new("profile-1-link")
    );
    let stdout = text(&output.stdout);
    assert!(stdout.contains("back at generation 1"), "{stdout}");
    assert!(
        stdout.ends_with("lock/ is not rolled back, so the next sync builds its pins again\n"),
        "{stdout}"
    );
    let call = fixture.nix_calls().pop().unwrap();
    assert_eq!(
        after(&call, "--profile"),
        fixture.state("profile").to_str().unwrap()
    );
}

#[test]
fn reloads_a_running_emacs_into_the_generation_it_rolls_back_to() {
    let fixture = synced_fixture();
    assert_exit(&fixture.upgrade().arg("--yes").output().unwrap(), 0);
    fixture.give_emacsclient("store-old");
    let _server = fixture.serve();
    let output = fixture
        .rollback()
        .env("FAKE_EMACSCLIENT", "reloaded")
        .output()
        .unwrap();
    assert_exit(&output, 0);
    let calls = fixture.emacsclient_calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert_eq!(
        Path::new(&calls[0][0]),
        fixture
            .path("store-old/bin/emacsclient")
            .canonicalize()
            .unwrap()
    );
    assert!(text(&output.stdout).ends_with("emx-reload: a report for reloaded\n"));
}

#[test]
fn fails_to_roll_back_past_the_first_generation() {
    let fixture = synced_fixture();
    let output = fixture.rollback().output().unwrap();
    assert_exit(&output, 1);
    assert!(text(&output.stderr).contains("nix failed"));
    assert_eq!(
        fs::read_link(fixture.state("profile")).unwrap(),
        Path::new("profile-1-link")
    );
}

#[test]
fn fails_to_roll_back_without_a_profile() {
    let fixture = Fixture::new();
    let output = fixture.rollback().output().unwrap();
    assert_exit(&output, 1);
    assert!(text(&output.stderr).contains("emx sync"));
    assert!(fixture.nix_calls().is_empty());
}

/// Moves a real lock with the real Nix while its one input can't be fetched,
/// so a change in how Nix words that failure fails here.
#[test]
#[ignore = "runs the real Nix; run with `cargo test -- --ignored`"]
fn names_what_the_real_nix_could_not_fetch() {
    const CREATE_UPSTREAM: &str = r#"
set -e
git init --quiet --bare --initial-branch=main "$1"
cd "$1"
tree=$(git hash-object -w -t tree /dev/null)
commit=$(git -c user.name=emx -c user.email=emx@example.org commit-tree -m one "$tree")
git update-ref refs/heads/main "$commit"
"#;
    let fixture = synced_fixture();
    let real_nix = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|dir| dir.join("nix"))
        .find(|nix| nix.exists())
        .unwrap();
    let upstream = fixture.path("upstream.git");
    let created = Command::new("sh")
        .args(["-c", CREATE_UPSTREAM, "sh"])
        .arg(&upstream)
        .output()
        .unwrap();
    assert_exit(&created, 0);

    let lock_dir = fixture.path("emx/lock");
    fs::write(
        lock_dir.join("flake.nix"),
        format!(
            r#"{{
  inputs.magit = {{ type = "git"; url = "file://{}"; flake = false; }};
  outputs = {{ ... }}: {{ }};
}}"#,
            upstream.display()
        ),
    )
    .unwrap();
    fs::remove_file(lock_dir.join("flake.lock")).unwrap();
    let locked = Command::new(&real_nix)
        .args(["--extra-experimental-features", "nix-command flakes"])
        .args(["flake", "lock"])
        .arg(&lock_dir)
        .env("XDG_CACHE_HOME", fixture.path("cache"))
        .output()
        .unwrap();
    assert_exit(&locked, 0);
    let pin = fs::read_to_string(lock_dir.join("flake.lock")).unwrap();

    // What's left is no repository, so Git's fetch fails and Nix goes on with
    // its cached clone.
    fs::rename(upstream.join("HEAD"), fixture.path("HEAD")).unwrap();
    let output = fixture
        .upgrade()
        .arg("--yes")
        .env("REAL_NIX", &real_nix)
        .env("XDG_CACHE_HOME", fixture.path("cache"))
        .output()
        .unwrap();
    assert_exit(&output, 1);
    let stderr = text(&output.stderr);
    assert!(stderr.contains("could not refresh magit"), "{stderr}");
    assert_eq!(
        fs::read_to_string(lock_dir.join("flake.lock")).unwrap(),
        pin
    );
}

/// This repo, which the real-Nix tests build.
fn this_repo() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}

/// Gives `command` the Emacs server directory `runtime`, a temporary one, so
/// that neither the emx nor the Emacs it runs reaches the user's Emacs server
/// or takes its socket.
fn own_server<'a>(command: &'a mut Command, runtime: &Path) -> &'a mut Command {
    command
        .env("XDG_RUNTIME_DIR", runtime)
        .env_remove("EMACS_SOCKET_NAME")
}

/// `emx` with the real Nix, a fake `systemctl` in `bin`, emx's profile under
/// `state` and the Emacs server directory `runtime`, building this repo. Extra
/// `nix build` arguments come from `EMX_TEST_NIX_ARGS`, split on whitespace.
fn real_emx(state: &Path, bin: &Path, runtime: &Path) -> Command {
    write_executable(&bin.join("systemctl"), FAKE_SYSTEMCTL);
    let mut command = Command::new(env!("CARGO_BIN_EXE_emx"));
    own_server(&mut command, runtime)
        .env("PATH", path_with(bin))
        .env("FAKE_SYSTEMCTL_ARGS", bin.join("systemctl-args"))
        .env("FAKE_SYSTEMCTL_STATE", "not-found")
        .env("XDG_STATE_HOME", state)
        .arg("--src")
        .arg(this_repo());
    command
}

/// Builds this repo's Emacs into emx's profile under `state`.
fn sync_this_repo(state: &Path, bin: &Path, runtime: &Path) {
    let extra = std::env::var("EMX_TEST_NIX_ARGS").unwrap_or_default();
    let output = real_emx(state, bin, runtime)
        .args(["sync", "--yes", "--"])
        .args(extra.split_whitespace())
        .status()
        .unwrap();
    assert!(output.success(), "emx sync failed: {output}");
}

/// A home in which emx's init loads: org-directory (the organisation module)
/// must exist.
fn emacs_home() -> TempDir {
    let home = tempfile::tempdir().unwrap();
    fs::create_dir_all(home.path().join("Documents/org")).unwrap();
    home
}

/// Gives the Emacs `command` runs the home `home`, with its XDG cache, config
/// and data directories in it, so that it neither reads nor writes the user's,
/// as emx's init writes under the cache directory.
fn in_home<'a>(command: &'a mut Command, home: &Path) -> &'a mut Command {
    command
        .env("HOME", home)
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
}

/// Builds this repo's Emacs with the real Nix into a temporary profile and
/// boots it with the real init.
#[test]
#[ignore = "builds emx's Emacs with Nix; run with `cargo test -- --ignored`"]
fn syncs_and_boots_this_repo() {
    let state = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    let runtime = tempfile::tempdir().unwrap();
    sync_this_repo(state.path(), bin.path(), runtime.path());

    let emacs = state.path().join("emx/profile/bin/emacs");
    let home = emacs_home();
    // emx's init starts a server.
    let boot = in_home(
        own_server(&mut Command::new(&emacs), runtime.path()),
        home.path(),
    )
    .arg("--batch")
    .arg("-u")
    .arg("")
    .arg(format!("--init-directory={}", this_repo().display()))
    .arg("--eval")
    .arg("(kill-emacs (if init-file-had-error 1 0))")
    .output()
    .unwrap();
    assert_exit(&boot, 0);
}

/// An Emacs daemon, killed when dropped.
struct Daemon(std::process::Child);

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Builds this repo's Emacs, starts a daemon on it with the real init, then
/// rolls back from a second generation of the same build. The rollback
/// reloads that daemon through the real `emacsclient` and emx-reload.
#[test]
#[ignore = "builds emx's Emacs with Nix; run with `cargo test -- --ignored`"]
fn reloads_a_real_running_emacs() {
    let state = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    let runtime = tempfile::tempdir().unwrap();
    sync_this_repo(state.path(), bin.path(), runtime.path());
    let profiles = state.path().join("emx");
    symlink(
        fs::read_link(profiles.join("profile-1-link")).unwrap(),
        profiles.join("profile-2-link"),
    )
    .unwrap();
    fs::remove_file(profiles.join("profile")).unwrap();
    symlink("profile-2-link", profiles.join("profile")).unwrap();

    let home = emacs_home();
    let socket = runtime.path().join("emacs/server");
    let _daemon = Daemon(
        in_home(
            own_server(
                &mut Command::new(profiles.join("profile/bin/emacs")),
                runtime.path(),
            ),
            home.path(),
        )
        .env("XDG_STATE_HOME", state.path())
        .env_remove("NOTIFY_SOCKET")
        .arg("--fg-daemon")
        .arg(format!("--init-directory={}", this_repo().display()))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap(),
    );
    // The server can answer before init finishes, so wait for both.
    let initialised = || {
        Command::new(profiles.join("profile/bin/emacsclient"))
            .arg(format!("--socket-name={}", socket.display()))
            .args(["--eval", "(and after-init-time t)"])
            .output()
            .is_ok_and(|output| text(&output.stdout) == "t\n")
    };
    let started = std::time::Instant::now();
    while !(socket.exists() && initialised()) {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(120),
            "the daemon didn't finish starting at {}",
            socket.display()
        );
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    let output = real_emx(state.path(), bin.path(), runtime.path())
        .arg("rollback")
        .output()
        .unwrap();
    assert_exit(&output, 0);
    let stdout = text(&output.stdout);
    assert!(stdout.contains("back at generation 1"), "{stdout}");
    assert!(
        stdout.contains("emx-reload: packages: same generation; configuration loaded"),
        "{stdout}"
    );
    assert!(!stdout.contains("restart"), "{stdout}");
}
