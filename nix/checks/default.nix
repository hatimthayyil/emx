# Checks that emx's Emacs is correct: `nix flake check`.
{
  pkgs,
  emacs,
  src,
}:
let
  packageList = pkgs.writeText "emx-packages" (
    pkgs.lib.concatLines (builtins.attrNames emacs.packageInputs)
  );
in
{
  # Boots with emx's real init, then loads every package in the set, including
  # dependencies. Batch mode loads init files only when -u names a user.
  # magit computes the git version when it loads.
  boot = pkgs.runCommandLocal "emx-boot-check" {
    EMX_PACKAGES = packageList;
    nativeBuildInputs = [ pkgs.git ];
  } ''
    export HOME=$TMPDIR
    # org-directory (modules/emx.organisation.el); org-node's :config fails
    # when it is missing.
    mkdir -p $HOME/Documents/org
    ${emacs}/bin/emacs --batch -u "" --init-directory=${src} -l ${./boot.el}
    touch $out
  '';

  # initialLibraries must list the libraries of the Emacs being built.
  builtin-libraries = pkgs.runCommandLocal "emx-builtin-libraries-check" { } ''
    if ! diff -u ${../emacs/builtin-libraries.txt} ${emacs.builtinLibraryList}; then
      echo "nix/emacs/builtin-libraries.txt does not match Emacs ${emacs.emacs.version}. Regenerate it:"
      echo '  cp $(nix build --no-link --print-out-paths .#emacs.builtinLibraryList) nix/emacs/builtin-libraries.txt'
      exit 1
    fi
    touch $out
  '';
}
