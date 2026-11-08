//! `emx sync`: builds the emx repo's Emacs and packages, shows what the build
//! changes, and after confirmation makes it a new generation of emx's
//! profile. The sync that creates the profile also starts the Emacs daemon;
//! later ones reload a running Emacs into the new generation (reload.rs).

use std::path::PathBuf;

use crate::build;
use crate::cli::SyncArgs;
use crate::daemon;
use crate::diff;
use crate::error::Error;
use crate::nix;
use crate::profile::Profile;
use crate::prompt;
use crate::reload;
use crate::source::Source;

/// The flake output holding Emacs with every package (nix/emacs).
const EMACS_OUTPUT: &str = "emacs";

/// What a sync did to emx's profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The profile already held this build.
    Unchanged,
    /// The build is the profile's first generation.
    Created,
    /// The build is a new generation.
    Switched,
}

pub fn run(src: Option<PathBuf>, args: &SyncArgs) -> Result<(), Error> {
    let source = Source::open(src)?;
    let profile = Profile::from_env()?;
    prompt::ensure_answerable(args.yes)?;
    let outcome = apply(&source, &profile, args)?;
    reload_if_switched(&profile, outcome)
}

/// Reloads a running Emacs when `outcome` is a new generation. The sync that
/// creates the profile starts the daemon on it instead.
pub fn reload_if_switched(profile: &Profile, outcome: Outcome) -> Result<(), Error> {
    if outcome == Outcome::Switched {
        reload::running_emacs(profile)?;
    }
    Ok(())
}

/// Builds `source` and, once confirmed, makes the build a generation of
/// `profile`. The caller has checked with [`prompt::ensure_answerable`].
pub fn apply(source: &Source, profile: &Profile, args: &SyncArgs) -> Result<Outcome, Error> {
    profile.create_parent()?;
    let current = profile.current()?;

    let build = build::build(
        &source.installable(EMACS_OUTPUT),
        &profile.pending_link(),
        &args.nix_args,
    )?;
    if let Some(current) = &current
        && current.store_path == build.store_path()
    {
        println!(
            "emx: nothing changed: generation {} is already {}",
            current.number,
            build.store_path().display()
        );
        return Ok(Outcome::Unchanged);
    }

    let shown = match &current {
        Some(current) => diff::show(&current.store_path, build.store_path()),
        None => diff::summarize(build.store_path()),
    };
    if let Err(err) = shown {
        eprintln!("emx: cannot show what the build changes: {err}");
    }
    if !args.yes && !prompt::confirm("Switch to this build?")? {
        return Err(Error::Declined {
            generation: current.map(|generation| generation.number),
        });
    }

    nix::run(&nix::set_profile(profile.link(), build.store_path()))?;
    let generation = profile.current()?.ok_or_else(|| Error::ProfileNotCreated {
        path: profile.link().to_owned(),
    })?;
    println!(
        "emx: {} is now generation {}: {}",
        profile.link().display(),
        generation.number,
        generation.store_path.display()
    );
    if current.is_some() {
        return Ok(Outcome::Switched);
    }
    match daemon::start_if_enabled() {
        Ok(true) => println!("emx: started the Emacs daemon"),
        Ok(false) => {}
        Err(err) => eprintln!("emx: cannot start the Emacs daemon: {err}"),
    }
    Ok(Outcome::Created)
}
