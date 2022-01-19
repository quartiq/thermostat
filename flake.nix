{
  description = "Firmware for the Sinara 8451 Thermostat";

  inputs.nixpkgs.url = github:NixOS/nixpkgs/nixos-21.11;
  inputs.mozilla-overlay = { url = github:mozilla/nixpkgs-mozilla; flake = false; };

  outputs = { self, nixpkgs, mozilla-overlay }:
    let
      pkgs = import nixpkgs { system = "x86_64-linux"; overlays = [ (import mozilla-overlay) ]; };
      rustManifest = pkgs.fetchurl {
        url = "https://static.rust-lang.org/dist/2020-10-30/channel-rust-nightly.toml";
        sha256 = "0iygcwzh8s0lfdghj5809krvzifc1ii1wm4sd3qqn7s0rz1s14hi";
      };

      targets = [
        "thumbv7em-none-eabihf"
      ];
      rustChannelOfTargets = _channel: _date: targets:
        (pkgs.lib.rustLib.fromManifestFile rustManifest {
          inherit (pkgs) stdenv lib fetchurl patchelf;
          }).rust.override {
          inherit targets;
          extensions = ["rust-src"];
        };
      rust = rustChannelOfTargets "nightly" null targets;
      rustPlatform = pkgs.recurseIntoAttrs (pkgs.makeRustPlatform {
        rustc = rust;
        cargo = rust;
      });
      thermostat = rustPlatform.buildRustPackage rec {
        name = "thermostat";
        version = "0.0.0";

        src = self;
        cargoLock = { 
          lockFile = ./Cargo.lock;
          outputHashes = {
            "stm32-eth-0.2.0" = "sha256-HXRr/NDhdIKqyjdA4D8ZmcO1dDpDawdlYPUOwcEbPQk=";
            "stm32f4xx-hal-0.8.3" = "sha256-MOv7tVtVMxr3IYMaN0Q8EQWxv3rubmCxjXMXuw/ZKAw=";
          };
        };

        nativeBuildInputs = [ pkgs.llvm ];

        buildPhase = ''
          cargo build --release --bin thermostat
        '';

        installPhase = ''
          mkdir -p $out $out/nix-support
          cp target/thumbv7em-none-eabihf/release/thermostat $out/thermostat.elf
          echo file binary-dist $out/thermostat.elf >> $out/nix-support/hydra-build-products
          llvm-objcopy -O binary target/thumbv7em-none-eabihf/release/thermostat $out/thermostat.bin
          echo file binary-dist $out/thermostat.bin >> $out/nix-support/hydra-build-products
        '';

        dontFixup = true;
      };
    in {
      packages.x86_64-linux = {
        inherit thermostat;
      };

      hydraJobs = {
        inherit thermostat;
      };

      devShell.x86_64-linux = pkgs.mkShell {
        name = "thermostat-dev-shell";
        buildInputs = with pkgs; [
          rustPlatform.rust.rustc
          rustPlatform.rust.cargo
          openocd dfu-util
          ] ++ (with python3Packages; [
            numpy matplotlib
          ]);
      };
      defaultPackage.x86_64-linux = thermostat;
    };
}