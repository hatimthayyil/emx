//! emx's commands run as a program. A fake `nix` on PATH records each call's
//! arguments and does what the real one would to the build root and the
//! profile, and a fake `systemctl` records its calls and answers as told, so
//! these tests need neither Nix nor the user's state directory or daemon. An
//! Emacs server is a socket the test listens on in its own runtime directory,
//! and a generation's fake `emacsclient` answers its reload as told. Each
//! package's upstream is local: a bare Git repository that Git's `insteadOf`
//! puts in place of its URL, and an ELPA archive's `archive-contents` file.
//! Their stdin is not a terminal, so emx never asks and needs `--yes`.

use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::os::unix::net::UnixListener;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, Instant};

use tempfile::TempDir;

/// Hands each call whose verb is in `REAL_NIX_VERBS`, a space-separated list,
/// to the Nix at `REAL_NIX`.
const FAKE_NIX: &str = r#"#!/bin/sh
printf '%s\n' "$@" -- >> "$FAKE_NIX_ARGS"
pwd >> "$FAKE_NIX_DIRS"
case " $REAL_NIX_VERBS " in *" $1 "*) exec "$REAL_NIX" "$@" ;; esac
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

/// Creates a bare Git repository at `$1` whose `main` holds one commit.
const CREATE_UPSTREAM: &str = r#"
set -e
git init --quiet --bare --initial-branch=main "$1"
cd "$1"
tree=$(git hash-object -w -t tree /dev/null)
commit=$(git -c user.name=emx -c user.email=emx@example.org commit-tree -m one "$tree")
git update-ref refs/heads/main "$commit"
"#;

/// Creates a bare Git repository at `$1` whose `main` holds one commit with
/// `foo.el`, the main file of a package named foo.
const CREATE_PACKAGE_UPSTREAM: &str = r#"
set -e
git init --quiet --bare --initial-branch=main "$1"
cd "$1"
blob=$(printf ';;; foo.el --- Foo  -*- lexical-binding: t -*-\n;; Version: 1.0\n(provide (quote foo))\n' | git hash-object -w --stdin)
tree=$(printf '100644 blob %s\tfoo.el\n' "$blob" | git mktree)
commit=$(git -c user.name=emx -c user.email=emx@example.org commit-tree -m one "$tree")
git update-ref refs/heads/main "$commit"
"#;

/// Adds a commit to `main` in the bare repository `$1`, with the same files.
const ADVANCE_UPSTREAM: &str = r#"
set -e
cd "$1"
commit=$(git -c user.name=emx -c user.email=emx@example.org commit-tree -m next -p main 'main^{tree}')
git update-ref refs/heads/main "$commit"
"#;

/// Runs `script` with `repo` as `$1` and Git's own defaults, and returns
/// `repo`'s commit on `main` after it.
fn git_script(script: &str, repo: &Path) -> String {
    let ran = Command::new("sh")
        .args(["-c", script, "sh"])
        .arg(repo)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert_exit(&ran, 0);
    let head = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "main"])
        .output()
        .unwrap();
    assert_exit(&head, 0);
    text(&head.stdout).trim().to_owned()
}

struct Fixture {
    root: TempDir,
}

impl Fixture {
    /// An emx repo pinning `gptel` from GitHub, `magit` from a Git URL and
    /// `denote` from an ELPA archive, each at what its upstream has.
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let gptel = git_script(
            CREATE_UPSTREAM,
            &root.path().join("github/karthink/gptel.git"),
        );
        let magit = git_script(CREATE_UPSTREAM, &root.path().join("example/magit.git"));
        fs::create_dir(root.path().join("elpa")).unwrap();
        let src = root.path().join("emx");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("flake.nix"), "").unwrap();
        fs::write(src.join("packages.el"), "").unwrap();
        fs::create_dir(src.join("lock")).unwrap();
        fs::write(
            src.join("lock/archive.lock"),
            format!(
                r#"{{"denote": {{"version": "4.0.0",
                    "inventory": {{"type": "archive", "url": "file://{}/elpa/"}}}}}}"#,
                root.path().display()
            ),
        )
        .unwrap();
        fs::write(
            src.join("lock/flake.lock"),
            format!(
                r#"{{"nodes": {{
                "root": {{"inputs": {{"gptel": "gptel", "magit": "magit"}}}},
                "gptel": {{"original": {{"type": "github", "owner": "karthink", "repo": "gptel"}},
                          "locked": {{"type": "github", "rev": "{gptel}"}}}},
                "magit": {{"original": {{"type": "git", "url": "https://example.org/magit.git"}},
                          "locked": {{"type": "git", "rev": "{magit}"}}}}
            }}}}"#
            ),
        )
        .unwrap();
        let fixture = Self { root };
        fixture.release_denote("4 0 0");
        let bin = fixture.path("bin");
        fs::create_dir(&bin).unwrap();
        write_executable(&bin.join("nix"), FAKE_NIX);
        write_executable(&bin.join("systemctl"), FAKE_SYSTEMCTL);
        fs::create_dir(fixture.path("state")).unwrap();
        fixture
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }

    /// Makes `version`, Elisp integers, denote's newest release in the
    /// archive.
    fn release_denote(&self, version: &str) {
        fs::write(
            self.path("elpa/archive-contents"),
            format!("(1 (denote . [({version}) nil \"Simple notes\" tar nil]))"),
        )
        .unwrap();
    }

    /// Adds a commit upstream of `gptel` or `magit`.
    fn advance(&self, package: &str) {
        let repo = match package {
            "gptel" => "github/karthink/gptel.git",
            "magit" => "example/magit.git",
            _ => panic!("no upstream for {package}"),
        };
        git_script(ADVANCE_UPSTREAM, &self.path(repo));
    }

    fn state(&self, name: &str) -> PathBuf {
        self.path("state").join("emx").join(name)
    }

    /// `emx` with an environment that reaches only the fixture's directories,
    /// building `store-emacs`, with no `emacs.service` and no Emacs server.
    /// Git config in the environment, which emx leaves in place, sends each
    /// upstream URL to the fixture's repositories.
    fn emx(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_emx"));
        let rewrites = [
            ("github", "https://github.com/"),
            ("example", "https://example.org/"),
        ];
        for (n, (dir, url)) in rewrites.iter().enumerate() {
            command
                .env(
                    format!("GIT_CONFIG_KEY_{n}"),
                    format!("url.file://{}/.insteadOf", self.path(dir).display()),
                )
                .env(format!("GIT_CONFIG_VALUE_{n}"), url);
        }
        command
            .env("GIT_CONFIG_COUNT", rewrites.len().to_string())
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
    assert!(calls[1].contains(&"--refresh".to_owned()));
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
    assert!(!calls[1].contains(&"--refresh".to_owned()));
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

/// Upstream moved past every pin, and the fake Nix moves none, as the real
/// one does when it can't fetch and keeps what it has.
#[test]
fn finishes_the_upgrade_then_fails_naming_what_it_could_not_refresh() {
    let fixture = synced_fixture();
    fixture.advance("gptel");
    fixture.advance("magit");
    fixture.release_denote("4 1 0");
    let output = fixture.upgrade().arg("--yes").output().unwrap();
    assert_exit(&output, 1);

    assert_eq!(nix_verbs(&fixture)[..3], ["update", "run", "build"]);
    assert_eq!(
        fs::read_link(fixture.state("profile")).unwrap(),
        Path::new("profile-2-link")
    );
    let stderr = text(&output.stderr);
    assert!(
        stderr.contains("not refreshed: denote, gptel, magit. Upstream has moved past their pins"),
        "{stderr}"
    );
    assert!(
        stderr.contains("running `emx upgrade` again clears this"),
        "{stderr}"
    );
    assert!(!stderr.contains("couldn't check"), "{stderr}");
}

#[test]
fn names_what_it_could_not_refresh_when_the_build_fails_too() {
    let fixture = synced_fixture();
    fixture.advance("magit");
    let output = fixture
        .upgrade()
        .arg("--yes")
        .env("FAKE_NIX_BUILD_STATUS", "3")
        .output()
        .unwrap();
    assert_exit(&output, 1);
    let stderr = text(&output.stderr);
    assert!(stderr.contains("not refreshed: magit."), "{stderr}");
    assert!(stderr.contains("nix failed"), "{stderr}");
    assert!(stderr.contains("`git restore lock`"), "{stderr}");
}

#[test]
fn finishes_the_upgrade_then_fails_naming_what_it_could_not_check() {
    let fixture = synced_fixture();
    fs::remove_dir_all(fixture.path("example/magit.git")).unwrap();
    fs::remove_file(fixture.path("elpa/archive-contents")).unwrap();
    let output = fixture.upgrade().arg("--yes").output().unwrap();
    assert_exit(&output, 1);

    assert_eq!(
        fs::read_link(fixture.state("profile")).unwrap(),
        Path::new("profile-2-link")
    );
    let stderr = text(&output.stderr);
    assert!(
        stderr.contains("couldn't check denote: curl file://"),
        "{stderr}"
    );
    assert!(
        stderr.contains("couldn't check magit: git ls-remote https://example.org/magit.git failed"),
        "{stderr}"
    );
    assert!(!stderr.contains("not refreshed"), "{stderr}");
    assert!(!stderr.contains("gptel"), "{stderr}");
}

#[test]
fn asks_upstream_only_about_the_named_packages() {
    let fixture = synced_fixture();
    fixture.advance("magit");
    fixture.release_denote("4 1 0");
    let output = fixture.upgrade().args(["gptel", "--yes"]).output().unwrap();
    assert_exit(&output, 0);
    assert!(!text(&output.stderr).contains("refreshed"));
}

/// A user's Git config could send emx's questions over SSH, or ask for
/// credentials, so emx asks without it.
#[test]
fn asks_upstream_without_the_users_git_config() {
    let fixture = synced_fixture();
    let config = fixture.path("gitconfig");
    fs::write(&config, "[protocol \"file\"]\n\tallow = never\n").unwrap();
    let output = fixture
        .upgrade()
        .arg("--yes")
        .env("GIT_CONFIG_GLOBAL", &config)
        .output()
        .unwrap();
    assert_exit(&output, 0);
}

/// A repository whose config sends gptel's upstream elsewhere.
fn misdirecting_repo(fixture: &Fixture) -> PathBuf {
    let repo = fixture.path("work");
    let made = Command::new("git")
        .args(["init", "--quiet"])
        .arg(&repo)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert_exit(&made, 0);
    fs::write(
        repo.join(".git/config"),
        format!(
            "[url \"file://{}/\"]\n\tinsteadOf = https://github.com/karthink/\n",
            fixture.path("missing").display()
        ),
    )
    .unwrap();
    repo
}

/// A repository emx is started in has config of its own, which emx's
/// questions don't follow.
#[test]
fn asks_upstream_without_the_config_of_the_repository_it_runs_in() {
    let fixture = synced_fixture();
    let output = fixture
        .upgrade()
        .arg("--yes")
        .current_dir(misdirecting_repo(&fixture))
        .output()
        .unwrap();
    assert_exit(&output, 0);
}

/// A Git hook, `git rebase --exec` or `git bisect run` names its repository
/// in `GIT_DIR`, whose config emx's questions don't follow either.
#[test]
fn asks_upstream_without_the_repository_in_git_dir() {
    let fixture = synced_fixture();
    let output = fixture
        .upgrade()
        .arg("--yes")
        .env("GIT_DIR", misdirecting_repo(&fixture).join(".git"))
        .output()
        .unwrap();
    assert_exit(&output, 0);
}

/// Each `git ls-remote` emx starts hangs on a `sleep` it started, which writes
/// its process ID to `FAKE_GIT_PIDS`, as git hangs on a remote helper stuck
/// on the network. A pipeline, since `sh` starts a `&` job ignoring SIGINT.
const HANGING_GIT: &str = r#"#!/bin/sh
sh -c 'echo $$ >> "$FAKE_GIT_PIDS"; exec sleep 300' | cat
"#;

/// Ctrl-C at a terminal interrupts its foreground process group, which emx
/// is in and git, in a group of its own, isn't: emx passes it on, so neither
/// git nor what git started outlives emx.
#[test]
fn stops_what_it_asked_upstream_with_when_interrupted() {
    let fixture = synced_fixture();
    write_executable(&fixture.path("bin/git"), HANGING_GIT);
    let pids = fixture.path("git-pids");
    let mut emx = fixture
        .upgrade()
        .args(["gptel", "magit", "--yes"])
        .env("FAKE_GIT_PIDS", &pids)
        .process_group(0)
        .spawn()
        .unwrap();
    let started = |count| {
        fs::read_to_string(&pids)
            .map(|text| text.lines().count() >= count)
            .unwrap_or(false)
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while !started(2) {
        assert!(Instant::now() < deadline, "emx started no git");
        thread::sleep(Duration::from_millis(20));
    }
    let group = libc::pid_t::try_from(emx.id()).unwrap();
    // SAFETY: kill only reads its arguments.
    assert_eq!(unsafe { libc::kill(-group, libc::SIGINT) }, 0);
    let status = emx.wait().unwrap();
    assert_eq!(status.signal(), Some(libc::SIGINT), "{status}");
    for pid in fs::read_to_string(&pids).unwrap().lines() {
        let pid: libc::pid_t = pid.parse().unwrap();
        // Its new parent reaps it soon after it dies.
        let deadline = Instant::now() + Duration::from_secs(5);
        // SAFETY: signal 0 only checks that the process exists.
        while unsafe { libc::kill(pid, 0) } == 0 {
            assert!(Instant::now() < deadline, "sleep {pid} outlived emx");
            thread::sleep(Duration::from_millis(20));
        }
    }
}

/// Under `nohup`, SIGHUP is ignored, and it stays ignored for emx and for
/// what emx starts, so closing the terminal stops neither.
#[test]
fn keeps_ignoring_the_hangup_nohup_ignores() {
    let fixture = synced_fixture();
    write_executable(&fixture.path("bin/git"), HANGING_GIT);
    let pids = fixture.path("git-pids");
    let mut upgrade = fixture.upgrade();
    upgrade
        .args(["gptel", "magit", "--yes"])
        .env("FAKE_GIT_PIDS", &pids)
        .process_group(0);
    // SAFETY: signal is async-signal-safe, as pre_exec requires.
    unsafe {
        upgrade.pre_exec(|| {
            libc::signal(libc::SIGHUP, libc::SIG_IGN);
            Ok(())
        });
    }
    let mut emx = upgrade.spawn().unwrap();
    let started = || {
        fs::read_to_string(&pids)
            .map(|text| text.lines().count() >= 2)
            .unwrap_or(false)
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while !started() {
        assert!(Instant::now() < deadline, "emx started no git");
        thread::sleep(Duration::from_millis(20));
    }
    let gits: Vec<libc::pid_t> = fs::read_to_string(&pids)
        .unwrap()
        .lines()
        .map(|pid| pid.parse().unwrap())
        .collect();
    let group = libc::pid_t::try_from(emx.id()).unwrap();
    // SAFETY: kill only reads its arguments.
    assert_eq!(unsafe { libc::kill(-group, libc::SIGHUP) }, 0);
    for &pid in &gits {
        // SAFETY: kill only reads its arguments.
        assert_eq!(unsafe { libc::kill(pid, libc::SIGHUP) }, 0);
    }
    thread::sleep(Duration::from_millis(300));
    assert!(emx.try_wait().unwrap().is_none(), "emx died of SIGHUP");
    for &pid in &gits {
        // SAFETY: signal 0 only checks that the process exists.
        assert_eq!(
            unsafe { libc::kill(pid, 0) },
            0,
            "sleep {pid} died of SIGHUP"
        );
    }
    // SAFETY: kill only reads its arguments.
    assert_eq!(unsafe { libc::kill(-group, libc::SIGINT) }, 0);
    let status = emx.wait().unwrap();
    assert_eq!(status.signal(), Some(libc::SIGINT), "{status}");
}

/// A `.curlrc` could send what curl fetches elsewhere, here to /dev/null, so
/// emx's curl doesn't read one.
#[test]
fn reads_archives_without_the_users_curlrc() {
    let fixture = synced_fixture();
    let home = fixture.path("curl-home");
    fs::create_dir(&home).unwrap();
    fs::write(home.join(".curlrc"), "output = \"/dev/null\"\n").unwrap();
    let output = fixture
        .upgrade()
        .arg("--yes")
        .env("CURL_HOME", &home)
        .output()
        .unwrap();
    assert_exit(&output, 0);
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

/// Moves a real lock with the real Nix and asks the real Git: once while Nix
/// can fetch the upstream, once while Git lists a commit Nix can't fetch and
/// Nix keeps its cached clone, and once while the upstream is gone for both.
#[test]
#[ignore = "runs the real Nix; run with `cargo test -- --ignored`"]
fn names_what_the_real_nix_could_not_refresh() {
    let fixture = synced_fixture();
    let real_nix = real_nix();
    let upstream = fixture.path("upstream.git");
    git_script(CREATE_UPSTREAM, &upstream);
    let upgrade = || {
        fixture
            .upgrade()
            .args(["magit", "--yes"])
            .env("REAL_NIX", &real_nix)
            .env("REAL_NIX_VERBS", "flake")
            .env("XDG_CACHE_HOME", fixture.path("cache"))
            .output()
            .unwrap()
    };

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
    let pin = || fs::read_to_string(lock_dir.join("flake.lock")).unwrap();
    let first = pin();

    // `main` names a commit whose object is missing: Git lists it, and Nix
    // can't fetch it.
    let next = git_script(ADVANCE_UPSTREAM, &upstream);
    let object = upstream.join("objects").join(&next[..2]).join(&next[2..]);
    fs::rename(&object, fixture.path("object")).unwrap();
    let output = upgrade();
    assert_exit(&output, 1);
    let stderr = text(&output.stderr);
    assert!(stderr.contains("not refreshed: magit."), "{stderr}");
    assert_eq!(pin(), first);

    // With the object back, Nix moves the pin to it, and nothing is stale.
    fs::rename(fixture.path("object"), &object).unwrap();
    let output = upgrade();
    assert_exit(&output, 0);
    assert!(pin().contains(&next), "{}", pin());
    let moved = pin();

    // What's left is no repository, so Git's fetch fails, Nix goes on with
    // its cached clone, and ls-remote can't ask.
    fs::rename(upstream.join("HEAD"), fixture.path("HEAD")).unwrap();
    let output = upgrade();
    assert_exit(&output, 1);
    let stderr = text(&output.stderr);
    assert!(
        stderr.contains("couldn't check magit: git ls-remote file://"),
        "{stderr}"
    );
    assert_eq!(pin(), moved);
}

/// Puts denote `version` in the fixture's ELPA archive as the tarball an
/// archive serves. `release_denote` then lists it in `archive-contents`.
const PUBLISH_DENOTE: &str = r#"
set -e
dir="$1/denote-$2"
mkdir "$dir"
printf ';;; denote.el --- Notes  -*- lexical-binding: t -*-\n;; Version: %s\n(provide (quote denote))\n' "$2" > "$dir/denote.el"
printf '(define-package "denote" "%s" "Notes" nil)\n' "$2" > "$dir/denote-pkg.el"
tar -C "$1" -cf "$dir.tar" "denote-$2"
rm -r "$dir"
"#;

/// An emx repo's flake reduced to its lock and update apps: twist as this
/// repo pins it, building `foo` from `recipes/` and `denote` from the ELPA
/// archive at `{elpa}`.
const TWIST_FLAKE: &str = r#"{
  inputs.emx.url = "{emx}";
  outputs = { emx, ... }:
    let
      pkgs = emx.inputs.nixpkgs.legacyPackages.x86_64-linux;
      env = emx.inputs.twist.lib.makeEnv {
        inherit pkgs;
        initFiles = [ ./packages.el ];
        lockDir = ./lock;
        initialLibraries = [ ];
        registries = [
          { name = "recipes"; type = "melpa"; path = ./recipes; }
          { name = "elpa"; type = "archive"; url = "file://{elpa}/"; }
        ];
      };
    in
    {
      legacyPackages.x86_64-linux = builtins.mapAttrs
        (name: app: pkgs.writeShellScriptBin name ''exec ${app.program} "$@"'')
        (env.makeApps { lockDirName = "lock"; });
    };
}
"#;

/// A named upgrade moves only the packages it names: the lock app it runs
/// keeps denote's ELPA pin, though the archive has a newer release, while the
/// named git package moves.
#[test]
#[ignore = "runs the real Nix; run with `cargo test -- --ignored`"]
fn keeps_elpa_pins_through_a_named_upgrade() {
    let fixture = synced_fixture();
    let real_nix = real_nix();
    let src = fixture.path("emx");
    let elpa = fixture.path("elpa");
    let foo = fixture.path("foo.git");
    git_script(CREATE_PACKAGE_UPSTREAM, &foo);
    fs::remove_dir_all(src.join("lock")).unwrap();
    fs::create_dir(src.join("lock")).unwrap();
    fs::create_dir(src.join("recipes")).unwrap();
    fs::write(
        src.join("recipes/foo"),
        format!(r#"(foo :fetcher git :url "file://{}")"#, foo.display()),
    )
    .unwrap();
    fs::write(
        src.join("packages.el"),
        "(use-package foo :ensure t)\n(use-package denote :ensure t)\n",
    )
    .unwrap();
    fs::write(
        src.join("flake.nix"),
        TWIST_FLAKE
            .replace("{emx}", this_repo().to_str().unwrap())
            .replace("{elpa}", elpa.to_str().unwrap()),
    )
    .unwrap();
    let publish = |version: &str| {
        let ran = Command::new("sh")
            .args(["-c", PUBLISH_DENOTE, "sh"])
            .arg(&elpa)
            .arg(version)
            .output()
            .unwrap();
        assert_exit(&ran, 0);
    };
    publish("4.0.0");
    fixture.release_denote("4 0 0");
    let locked = Command::new(&real_nix)
        .args(["--extra-experimental-features", "nix-command flakes"])
        .args(["run", "--impure"])
        .arg(format!("{}#lock", src.display()))
        .current_dir(&src)
        .env("XDG_CACHE_HOME", fixture.path("cache"))
        .output()
        .unwrap();
    assert_exit(&locked, 0);
    let read = |name: &str| fs::read_to_string(src.join("lock").join(name)).unwrap();
    assert!(
        read("archive.lock").contains(r#""version": "4.0.0""#),
        "{}",
        read("archive.lock")
    );

    publish("4.1.0");
    fixture.release_denote("4 1 0");
    let next = git_script(ADVANCE_UPSTREAM, &foo);
    let output = fixture
        .upgrade()
        .args(["foo", "--yes"])
        .env("REAL_NIX", &real_nix)
        .env("REAL_NIX_VERBS", "flake run")
        .env("XDG_CACHE_HOME", fixture.path("cache"))
        // Nix would otherwise answer with the archive-contents it read for
        // the first lock.
        .env("NIX_CONFIG", "tarball-ttl = 0")
        .output()
        .unwrap();
    assert_exit(&output, 0);
    assert!(read("flake.lock").contains(&next), "{}", read("flake.lock"));
    let archive = read("archive.lock");
    assert!(archive.contains(r#""version": "4.0.0""#), "{archive}");
    assert!(!archive.contains("4.1.0"), "{archive}");
}

/// The real Nix, the first on PATH.
fn real_nix() -> PathBuf {
    std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|dir| dir.join("nix"))
        .find(|nix| nix.exists())
        .unwrap()
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
