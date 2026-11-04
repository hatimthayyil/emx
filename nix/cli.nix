# The emx CLI (../cli), which builds emx's Emacs into emx's profile.
{ lib, rustPlatform }:
let
  manifest = (lib.importTOML ../cli/Cargo.toml).package;
in
rustPlatform.buildRustPackage {
  pname = manifest.name;
  inherit (manifest) version;
  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../cli
    ];
  };
  cargoLock.lockFile = ../Cargo.lock;
  meta = {
    inherit (manifest) description;
    mainProgram = "emx";
  };
}
