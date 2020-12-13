{ mozillaOverlay ? builtins.fetchTarball "https://github.com/mozilla/nixpkgs-mozilla/archive/master.tar.gz"
}:
let
  pkgs = import <nixpkgs> {
    overlays = [ (import mozillaOverlay) ];
  };
  rust = pkgs.rustChannelOfTargets "nightly" null [ "thumbv7em-none-eabihf" ];
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
