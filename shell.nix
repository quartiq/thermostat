{ mozillaOverlay ? builtins.fetchTarball "https://github.com/mozilla/nixpkgs-mozilla/archive/master.tar.gz",
  latestRustNightly ? false,
}:
let
  pkgs = import <nixpkgs> {
    overlays = [ (import mozillaOverlay) ];
  };
  rust =
    if latestRustNightly
    then pkgs.rustChannelOfTargets "nightly" null [ "thumbv7em-none-eabihf" ]
    else (pkgs.recurseIntoAttrs (
      pkgs.callPackage (import <nix-scripts/stm32/rustPlatform.nix>) {}
    )).rust.cargo;
in
pkgs.mkShell {
  name = "thermostat-env";
  buildInputs = with pkgs; [
    rust gcc
    openocd dfu-util
  ] ++ (with python3Packages; [
    numpy matplotlib
  ]);
}
