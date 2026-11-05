# The emx CLI (../cli), which builds emx's Emacs into emx's profile.
{ lib
, makeWrapper
, nix-output-monitor
, pkg-config
, rustPlatform
, sqlite
}:
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
  nativeBuildInputs = [ makeWrapper pkg-config ];
  # dix reads the Nix store database through SQLite.
  buildInputs = [ sqlite ];
  # `emx sync` renders build logs with nom.
  postInstall = ''
    wrapProgram $out/bin/emx --prefix PATH : ${lib.makeBinPath [ nix-output-monitor ]}
  '';
  meta = {
    inherit (manifest) description;
    mainProgram = "emx";
    # The source is Apache-2.0; linking dix makes the built package GPL-3.0-only.
    license = lib.licenses.gpl3Only;
  };
}
