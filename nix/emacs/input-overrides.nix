# Dependencies that packages use without declaring them in Package-Requires.
{
  acm = _: prev: {
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
}
