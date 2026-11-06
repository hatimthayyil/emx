//! emx's commands run as a program. A fake `nix` on PATH records each call's
//! arguments and does what the real one would to the build root and the
//! profile, and a fake `systemctl` records its calls and answers as told, so
//! these tests need neither Nix nor the user's state directory or daemon.
//! Their stdin is not a terminal, so emx never asks and needs `--yes`.

use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
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
    /// building `store-emacs`, with no `emacs.service`.
    fn emx(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_emx"));
        command
            .env_remove("EMX_SRC")
            .env("PATH", path_with(&self.path("bin")))
            .env("HOME", self.path("home"))
            .env("XDG_STATE_HOME", self.path("state"))
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
fn asks_for_a_restart_after_switching() {
    let fixture = synced_fixture();
    let output = fixture.upgrade().arg("--yes").output().unwrap();
    assert_exit(&output, 0);
    assert_eq!(
        fs::read_link(fixture.state("profile")).unwrap(),
        Path::new("profile-2-link")
    );
    let stdout = text(&output.stdout);
    assert!(
        stdout.ends_with("systemctl --user restart emacs\n"),
        "{stdout}"
    );
}

#[test]
fn asks_for_no_restart_when_nothing_changed() {
    let fixture = synced_fixture();
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
}

#[test]
fn asks_for_no_restart_after_creating_the_profile() {
    let fixture = Fixture::new();
    let output = fixture.upgrade().arg("--yes").output().unwrap();
    assert_exit(&output, 0);
    assert!(!text(&output.stdout).contains("restart"));
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
    assert!(text(&output.stdout).ends_with("systemctl --user restart emacs\n"));
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
    assert!(stdout.contains("lock/ is not rolled back"), "{stdout}");
    assert!(
        stdout.ends_with("systemctl --user restart emacs\n"),
        "{stdout}"
    );
    let call = fixture.nix_calls().pop().unwrap();
    assert_eq!(
        after(&call, "--profile"),
        fixture.state("profile").to_str().unwrap()
    );
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

/// Builds this repo's Emacs with the real Nix into a temporary profile and
/// boots it with the real init. Extra `nix build` arguments come from
/// `EMX_TEST_NIX_ARGS`, split on whitespace.
#[test]
#[ignore = "builds emx's Emacs with Nix; run with `cargo test -- --ignored`"]
fn syncs_and_boots_this_repo() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let state = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    write_executable(&bin.path().join("systemctl"), FAKE_SYSTEMCTL);
    let extra = std::env::var("EMX_TEST_NIX_ARGS").unwrap_or_default();
    let output = Command::new(env!("CARGO_BIN_EXE_emx"))
        .env("PATH", path_with(bin.path()))
        .env("FAKE_SYSTEMCTL_ARGS", bin.path().join("systemctl-args"))
        .env("FAKE_SYSTEMCTL_STATE", "not-found")
        .env("XDG_STATE_HOME", state.path())
        .arg("--src")
        .arg(src)
        .args(["sync", "--yes", "--"])
        .args(extra.split_whitespace())
        .status()
        .unwrap();
    assert!(output.success(), "emx sync failed: {output}");

    let emacs = state.path().join("emx/profile/bin/emacs");
    let home = tempfile::tempdir().unwrap();
    // org-directory (the organisation module) must exist.
    fs::create_dir_all(home.path().join("Documents/org")).unwrap();
    let boot = Command::new(&emacs)
        .env("HOME", home.path())
        .arg("--batch")
        .arg("-u")
        .arg("")
        .arg(format!("--init-directory={}", src.display()))
        .arg("--eval")
        .arg("(kill-emacs (if init-file-had-error 1 0))")
        .output()
        .unwrap();
    assert_exit(&boot, 0);
}
