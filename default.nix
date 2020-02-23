let
  moz_overlay = import (builtins.fetchTarball {
    url = "https://github.com/mozilla/nixpkgs-mozilla/archive/e912ed483e980dfb4666ae0ed17845c4220e5e7c.tar.gz";
    sha256 = "08fvzb8w80bkkabc1iyhzd15f4sm7ra10jn32kfch5klgl0gj3j3";
  } + "/rust-overlay.nix");

  nixpkgs = import <nixpkgs> {
    overlays = [ moz_overlay ];
  };

  rustChannel = nixpkgs.rustChannelOf {
    rustToolchain = ./rust-toolchain;
  };
in
  nixpkgs.buildEnv {
    name = "liboverlay-dev";

    paths = [
      nixpkgs.gcc
      (rustChannel.rust.override {
        extensions = ["rust-src"];
      })
    ];
  }