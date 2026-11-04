//! `emx sync`: builds the checkout's Emacs and packages into a new generation
//! of emx's profile.

use std::path::PathBuf;

use crate::checkout::Checkout;
use crate::cli::SyncArgs;
use crate::error::Error;
use crate::nix;
use crate::profile::Profile;

/// The flake output holding Emacs with every package (nix/emacs).
const EMACS_OUTPUT: &str = "emacs";

pub fn run(checkout: Option<PathBuf>, args: &SyncArgs) -> Result<(), Error> {
    let checkout = Checkout::open(checkout)?;
    let profile = Profile::from_env()?;
    profile.create_parent()?;
    nix::run(&nix::build_into_profile(
        &checkout.installable(EMACS_OUTPUT),
        profile.link(),
        &args.nix_args,
    ))?;
    let generation = profile.current()?.ok_or_else(|| Error::ProfileNotCreated {
        path: profile.link().to_owned(),
    })?;
    println!(
        "emx: {} is now generation {}: {}",
        profile.link().display(),
        generation.number,
        generation.store_path.display()
    );
    Ok(())
}
