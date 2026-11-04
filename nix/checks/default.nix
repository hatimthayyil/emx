# Checks that emx's Emacs is correct: `nix flake check`.
{ pkgs
, emacs
, homeManager
, homeModule
, src
}:
let
  packageList = pkgs.writeText "emx-packages" (
    pkgs.lib.concatLines (builtins.attrNames emacs.packageInputs)
  );

  home = homeManager.lib.homeManagerConfiguration {
    inherit pkgs;
    modules = [
      homeModule
      {
        home = {
          username = "emx";
          homeDirectory = "/home/emx";
          stateVersion = "26.05";
        };
        programs.emx = {
          enable = true;
          src = "/home/emx/code/emx";
          defaultEditor = true;
        };
        services.emacs.enable = true;
      }
    ];
  };
in
{
  # Boots with emx's real init, then loads every package in the set, including
  # dependencies. Batch mode loads init files only when -u names a user.
  # magit computes the git version when it loads.
  boot = pkgs.runCommandLocal "emx-boot-check"
    {
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

  # Builds a home-manager configuration using the module, and fails if its
  # closure holds emx's package set or Emacs, which only `emx sync` builds, or
  # if it lacks the desktop entry's icon.
  home-module =
    pkgs.runCommandLocal "emx-home-module-check"
      {
        exportReferencesGraph = [
          "closure"
          home.activationPackage
        ];
      }
      ''
        for path in ${builtins.unsafeDiscardStringContext emacs.outPath} \
                    ${builtins.unsafeDiscardStringContext emacs.emacs.outPath}; do
          if grep -qF "$path" closure; then
            echo "the home-manager module depends on $path"
            exit 1
          fi
        done
        if [ ! -e ${home.config.home.path}/share/icons/hicolor/scalable/apps/emacs.svg ]; then
          echo "the home-manager generation lacks Emacs's icon"
          exit 1
        fi
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
