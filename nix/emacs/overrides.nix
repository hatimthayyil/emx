# Build fixes and native parts that twist leaves to the user.
{ pkgs }:
_: prev: {
  # Its byte-compilation changes into HOME.
  auctex = prev.auctex.overrideAttrs (old: {
    preBuild = "export HOME=$TMPDIR\n" + (old.preBuild or "");
  });

  jinx = prev.jinx.overrideAttrs (old: {
    nativeBuildInputs = (old.nativeBuildInputs or [ ]) ++ [ pkgs.pkg-config ];
    buildInputs = old.buildInputs ++ [ pkgs.enchant_2 ];
    preBuild = ''
      $CC -O2 -fPIC -shared $(pkg-config --cflags enchant-2) -o jinx-mod.so jinx-mod.c -lenchant-2
      rm jinx-mod.c emacs-module.h
    '';
  });

  lsp-bridge = prev.lsp-bridge.overrideAttrs (_: {
    postPatch = ''
      substituteInPlace lsp-bridge.el --replace-fail '"python-lsp-bridge")' '"${
        pkgs.python3.withPackages (ps: [
          ps.epc
          ps.orjson
          ps.packaging
          ps.paramiko
          ps.rapidfuzz
          ps.setuptools
          ps.sexpdata
          ps.six
          ps.watchdog
        ])
      }/bin/python3")'
    '';
  });

  pdf-tools = prev.pdf-tools.overrideAttrs (old: {
    nativeBuildInputs = (old.nativeBuildInputs or [ ]) ++ [
      pkgs.autoconf
      pkgs.automake
      pkgs.pkg-config
    ];
    buildInputs = old.buildInputs ++ [
      pkgs.libpng
      pkgs.poppler
      pkgs.zlib
    ];
    env.CXXFLAGS = "-std=c++17";
    preBuild = ''
      make -C build server/epdfinfo
      cp build/server/epdfinfo .
      rm -rf build
    '';
  });

  # It requires sly-stickers, which sly keeps in contrib/, off the build load-path.
  sly-stepper = prev.sly-stepper.overrideAttrs (_: {
    dontByteCompile = true;
  });

  vterm = prev.vterm.overrideAttrs (old: {
    nativeBuildInputs = (old.nativeBuildInputs or [ ]) ++ [ pkgs.cmake ];
    buildInputs = old.buildInputs ++ [ pkgs.libvterm-neovim ];
    preBuild = ''
      cmake -S . -B cmake-build -DUSE_SYSTEM_LIBVTERM=ON
      make -C cmake-build
      rm -rf cmake-build CMakeLists.txt *.c *.h
    '';
  });
}
