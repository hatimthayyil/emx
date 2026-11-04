//! `emx sync` run as a program. A fake `nix` on PATH records its arguments and
//! creates the profile generation the real `nix build --profile` would, so
//! these tests need neither Nix nor the user's state directory.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

const FAKE_NIX: &str = r#"#!/bin/sh
printf '%s\n' "$@" > "$FAKE_NIX_ARGS"
[ "${FAKE_NIX_STATUS:-0}" = 0 ] || exit "$FAKE_NIX_STATUS"
while [ $# -gt 0 ]; do
  [ "$1" = --profile ] && profile=$2
  shift
done
mkdir -p "$FAKE_NIX_OUT"
ln -s "$FAKE_NIX_OUT" "$profile-1-link"
ln -s "$(basename "$profile")-1-link" "$profile"
"#;

struct Fixture {
    root: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let checkout = root.path().join("emx");
        fs::create_dir(&checkout).unwrap();
        fs::write(checkout.join("flake.nix"), "").unwrap();
        fs::write(checkout.join("packages.el"), "").unwrap();
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

    /// `emx` with an environment that reaches only the fixture's directories.
    fn emx(&self) -> Command {
        let path = std::env::join_paths(std::iter::once(self.path("bin")).chain(
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
        ))
        .unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_emx"));
        command
            .env_remove("EMX_CHECKOUT")
            .env("PATH", path)
            .env("HOME", self.path("home"))
            .env("XDG_STATE_HOME", self.path("state"))
            .env("FAKE_NIX_ARGS", self.path("nix-args"))
            .env("FAKE_NIX_OUT", self.path("store-emacs"));
        command
    }

    fn nix_args(&self) -> Vec<String> {
        fs::read_to_string(self.path("nix-args"))
            .unwrap()
            .lines()
            .map(str::to_owned)
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

#[test]
fn builds_the_checkout_into_a_new_profile_generation() {
    let fixture = Fixture::new();
    let output = fixture
        .emx()
        .env("EMX_CHECKOUT", fixture.path("emx"))
        .args(["sync", "--", "--max-jobs", "2"])
        .output()
        .unwrap();
    assert_exit(&output, 0);

    let profile = fixture.path("state").join("emx").join("profile");
    let checkout = fixture.path("emx").canonicalize().unwrap();
    let args = fixture.nix_args();
    assert_eq!(args[0], "build");
    assert!(args.contains(&"--no-update-lock-file".to_owned()));
    let at = args.iter().position(|a| a == "--profile").unwrap();
    assert_eq!(Path::new(&args[at + 1]), profile);
    assert_eq!(args[at + 2], format!("{}#emacs", checkout.display()));
    assert_eq!(args[args.len() - 2..], ["--max-jobs", "2"]);

    assert_eq!(
        text(&output.stdout),
        format!(
            "emx: {} is now generation 1: {}\n",
            profile.display(),
            fixture.path("store-emacs").display()
        )
    );
}

#[test]
fn takes_the_checkout_from_the_command_line() {
    let fixture = Fixture::new();
    let output = fixture
        .emx()
        .arg("--checkout")
        .arg(fixture.path("emx"))
        .arg("sync")
        .output()
        .unwrap();
    assert_exit(&output, 0);
}

#[test]
fn fails_without_a_checkout() {
    let fixture = Fixture::new();
    let output = fixture.emx().arg("sync").output().unwrap();
    assert_exit(&output, 1);
    assert!(text(&output.stderr).contains("EMX_CHECKOUT"));
    assert!(!fixture.path("nix-args").exists());
}

#[test]
fn fails_when_the_checkout_is_not_emx() {
    let fixture = Fixture::new();
    let output = fixture
        .emx()
        .env("EMX_CHECKOUT", fixture.path("state"))
        .arg("sync")
        .output()
        .unwrap();
    assert_exit(&output, 1);
    assert!(text(&output.stderr).contains("is not an emx checkout"));
}

#[test]
fn reports_a_failed_build() {
    let fixture = Fixture::new();
    let output = fixture
        .emx()
        .env("EMX_CHECKOUT", fixture.path("emx"))
        .env("FAKE_NIX_STATUS", "3")
        .arg("sync")
        .output()
        .unwrap();
    assert_exit(&output, 1);
    assert!(text(&output.stderr).contains("nix build failed"));
    assert!(!fixture.path("state").join("emx").join("profile").exists());
}

#[test]
fn reports_missing_nix() {
    let fixture = Fixture::new();
    let output = fixture
        .emx()
        .env("EMX_CHECKOUT", fixture.path("emx"))
        .env("PATH", fixture.path("empty"))
        .arg("sync")
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

/// Builds this checkout's Emacs with the real Nix into a temporary profile and
/// boots it with the real init. Extra `nix build` arguments come from
/// `EMX_TEST_NIX_ARGS`, split on whitespace.
#[test]
#[ignore = "builds emx's Emacs with Nix; run with `cargo test -- --ignored`"]
fn syncs_and_boots_this_checkout() {
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let state = tempfile::tempdir().unwrap();
    let extra = std::env::var("EMX_TEST_NIX_ARGS").unwrap_or_default();
    let output = Command::new(env!("CARGO_BIN_EXE_emx"))
        .env("XDG_STATE_HOME", state.path())
        .arg("--checkout")
        .arg(checkout)
        .arg("sync")
        .arg("--")
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
        .arg(format!("--init-directory={}", checkout.display()))
        .arg("--eval")
        .arg("(kill-emacs (if init-file-had-error 1 0))")
        .output()
        .unwrap();
    assert_exit(&boot, 0);
}
