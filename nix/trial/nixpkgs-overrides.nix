# Packages that nixpkgs' Emacs package set lacks or marks broken.
{ pkgs }:
self: super:
let
  github =
    owner: repo: rev: hash:
    pkgs.fetchFromGitHub { inherit owner repo rev hash; };
in
{
  magit-stgit = super.magit-stgit.overrideAttrs (old: {
    meta = old.meta // { broken = false; };
  });

  docker-compose-mode = self.melpaBuild {
    pname = "docker-compose-mode";
    version = "1.1.0";
    src = github "meqif" "docker-compose-mode" "abaa4f3aeb5c62d7d16e186dd7d77f4e846e126a" "sha256-TfQ0r6bSjQglSyvz9OQVKmJSBRy37pDLqk/LwqPUdYg=";
    packageRequires = [
      self.dash
      # Used, but absent from its Package-Requires.
      self.f
      self.yaml-mode
    ];
  };

  erk = self.melpaBuild {
    pname = "erk";
    version = "0.5.0";
    src = github "positron-solutions" "elisp-repo-kit" "0d9906415a649caff2df7b4b1b3f8f6cc337032a" "sha256-7mcEB0fOCTyYC7fBP3R71MsQVf23jnSyIqoe+IdhnAU=";
    files = ''("lisp/*.el")'';
    packageRequires = [
      self.auto-compile
      self.dash
      self.license-templates
    ];
  };

  gptel-quick = self.melpaBuild {
    pname = "gptel-quick";
    version = "0.0.5";
    src = github "karthink" "gptel-quick" "36fe296e016449433fa1213f4b89cb8dc7d4db5e" "sha256-W2cEtjhoXxAhMxycLAg0qe2Ehpgn1L/m1VcpZu/Trsw=";
    packageRequires = [
      self.compat
      self.gptel
    ];
  };

  persid = self.melpaBuild {
    pname = "persid";
    version = "0.1.1";
    src = github "rougier" "persid" "cf84a4340bd3e5b16b4412e98c4243da9f72503a" "sha256-nhyMYf8DEnF3QQV40dSC5W/Vedvkj39+X/e5sPWQ0Zo=";
  };

  rainbow-csv = self.melpaBuild {
    pname = "rainbow-csv";
    version = "0.1.0";
    src = github "emacs-vs" "rainbow-csv" "5763375e48e870ffd284e051eb2428cac743d7e0" "sha256-NcFI38TnPHWMUdwX3UBZVjiPj6uCl0+WnIMNdzQ73G8=";
    packageRequires = [ self.csv-mode ];
  };

  sly-stepper = self.melpaBuild {
    pname = "sly-stepper";
    version = "0.1";
    src = github "joaotavora" "sly-stepper" "da84e3bba8466c2290c2dc7c27d7f4c48c27b39e" "sha256-gMbmG42gxaQrgDUVySbGGpayxDM3pKAgDvYpd9KZ4B4=";
    files = ''(:defaults "*.lisp" "*.asd")'';
    # It requires sly-stickers, which sly keeps in contrib/, off the build load-path.
    ignoreCompilationError = true;
    packageRequires = [ self.sly ];
  };
}
