//! `emx sync` run as a program. A fake `nix` on PATH records each call's
//! arguments and does what the real one would to the build root and the
//! profile, so these tests need neither Nix nor the user's state directory.
//! Their stdin is not a terminal, so emx never asks and needs `--yes`.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

const FAKE_NIX: &str = r#"#!/bin/sh
printf '%s\n' "$@" -- >> "$FAKE_NIX_ARGS"
[ "${FAKE_NIX_STATUS:-0}" = 0 ] || exit "$FAKE_NIX_STATUS"
link= profile= target=
while [ $# -gt 0 ]; do
  case "$1" in
    --out-link) link=$2; shift ;;
    --profile) profile=$2; shift ;;
    /*) target=$1 ;;
  esac
  shift
done
if [ -n "$link" ]; then
  mkdir -p "$FAKE_NIX_OUT"
  ln -sfn "$FAKE_NIX_OUT" "$link"
fi
if [ -n "$profile" ]; then
  n=1
  while [ -L "$profile-$n-link" ]; do n=$((n + 1)); done
  ln -s "$target" "$profile-$n-link"
  ln -sfn "$(basename "$profile")-$n-link" "$profile"
fi
"#;

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
        let bin = root.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let nix = bin.join("nix");
        fs::write(&nix, FAKE_NIX).unwrap();
        fs::set_permissions(&nix, fs::Permissions::from_mode(0o755)).unwrap();
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
    /// building `store-emacs`.
    fn emx(&self) -> Command {
        let path = std::env::join_paths(std::iter::once(self.path("bin")).chain(
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
        ))
        .unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_emx"));
        command
            .env_remove("EMX_SRC")
            .env("PATH", path)
            .env("HOME", self.path("home"))
            .env("XDG_STATE_HOME", self.path("state"))
            .env("FAKE_NIX_ARGS", self.path("nix-args"))
            .env("FAKE_NIX_OUT", self.path("store-emacs"));
        command
    }

    fn sync(&self) -> Command {
        let mut command = self.emx();
        command.env("EMX_SRC", self.path("emx")).arg("sync");
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
    assert!(text(&output.stderr).contains("nix build failed"));
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

/// Builds this repo's Emacs with the real Nix into a temporary profile and
/// boots it with the real init. Extra `nix build` arguments come from
/// `EMX_TEST_NIX_ARGS`, split on whitespace.
#[test]
#[ignore = "builds emx's Emacs with Nix; run with `cargo test -- --ignored`"]
fn syncs_and_boots_this_repo() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let state = tempfile::tempdir().unwrap();
    let extra = std::env::var("EMX_TEST_NIX_ARGS").unwrap_or_default();
    let output = Command::new(env!("CARGO_BIN_EXE_emx"))
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
    // org-directory (modules/emx.organisation.el) must exist.
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
