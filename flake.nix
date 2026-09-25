{
  description = "A post-modern text editor.";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    rust-overlay,
    ...
  }: let
    inherit (nixpkgs) lib;
    eachSystem = lib.genAttrs lib.systems.flakeExposed;
    pkgsFor = eachSystem (system:
      import nixpkgs {
        localSystem.system = system;
        overlays = [(import rust-overlay) self.overlays.mitos];
      });
    gitRev = self.rev or self.dirtyRev or null;
  in {
    packages = eachSystem (system: let
      pkgs = pkgsFor.${system};
      toolchain = pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      rustPlatform = pkgs.makeRustPlatform {
        cargo = toolchain;
        rustc = toolchain;
      };
    in rec {
      mitos = pkgs.mitos.override {inherit rustPlatform;};
      /*
      The default Mitos build. Uses the toolchain declared in rust-toolchain.toml
      and unstable nixpkgs.

      The build inputs can be overridden with the following:

      packages.${system}.default.override { rustPlatform = newPlatform; };

      Overriding a derivation attribute can be done as well:

      packages.${system}.default.overrideAttrs { buildType = "debug"; };
      */
      default = mitos;
    });
    checks =
      lib.mapAttrs (system: _: {
        mitos = self.packages.${system}.mitos;
      })
      pkgsFor;

    # Devshell behavior is preserved.
    devShells =
      lib.mapAttrs (system: pkgs: {
        default = let
          commonRustFlagsEnv = "-C link-arg=-fuse-ld=lld -C target-cpu=native --cfg tokio_unstable";
          platformRustFlagsEnv = lib.optionalString pkgs.stdenv.isLinux "-Clink-arg=-Wl,--no-rosegment";
        in
          pkgs.mkShell {
            inputsFrom = [
              (self.checks.${system}.mitos.override {
                includeGrammarIf = _: false;
              })
            ];
            nativeBuildInputs = with pkgs;
              [
                lld
                cargo-flamegraph
                rust-bin.nightly.latest.rust-analyzer
                mdbook
              ]
              ++ (lib.optional (stdenv.isx86_64 && stdenv.isLinux) cargo-tarpaulin)
              ++ (lib.optional stdenv.isLinux lldb);
            shellHook = ''
              export RUST_BACKTRACE="1"
              export RUSTFLAGS="''${RUSTFLAGS:-""} ${commonRustFlagsEnv} ${platformRustFlagsEnv}"
            '';
          };
      })
      pkgsFor;

    overlays = {
      mitos = final: prev: {
        mitos = final.callPackage ./default.nix {inherit gitRev;};
      };

      default = self.overlays.mitos;
    };
  };
}
