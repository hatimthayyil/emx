# Dependencies that packages use without declaring them in Package-Requires.
{
  acm = _: prev: {
    version = "0";
    packageRequires = prev.packageRequires // {
      yasnippet = "0";
    };
  };

  docker-compose-mode = _: prev: {
    packageRequires = prev.packageRequires // {
      f = "0";
    };
  };

  guix = _: prev: {
    packageRequires = prev.packageRequires // {
      geiser-guile = "0";
    };
  };

  lsp-bridge = _: prev: {
    packageRequires = prev.packageRequires // {
      acm = "0";
    };
  };

  # persistMetadata needs a version, and these packages have no Version header.
  howm = _: _: { version = "0"; };
  magit-popup = _: _: { version = "0"; };
  magit-stgit = _: _: { version = "0"; };
  notmuch = _: _: { version = "0"; };
  org-node = _: _: { version = "0"; };
}
