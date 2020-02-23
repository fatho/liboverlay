{ stdenv, rustc, ... }:
stdenv.mkDerivation {
  pname = "liboverlay";
  version = "0.1.0";

  src =
    let
      whitelist = map builtins.toString [
        ./src
        ./src/lib.rs
        ./src/config.rs
        ./src/redir.rs
      ];
    in
      builtins.filterSource (path: type: builtins.elem path whitelist) ./.;

  buildInputs = [ rustc ];

  buildPhase = ''
    mkdir out
    rustc \
      --edition=2018 \
      --crate-name overlay \
      src/lib.rs \
      --crate-type cdylib \
      -C opt-level=3 \
      --out-dir out
  '';

  installPhase = ''
    mkdir -p $out/lib
    mv out/liboverlay.so $out/lib
  '';
}