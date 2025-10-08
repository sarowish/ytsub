{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    crane.url = "github:ipetkov/crane";

    flake-utils.url = "github:numtide/flake-utils";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      crane,
      flake-utils,
      rust-overlay,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs {
          inherit system;
          overlays = [ (import rust-overlay) ];
        };

        rust-toolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [
            "rust-src"
            "rust-analyzer"
          ];
        };

        craneLib = (crane.mkLib pkgs).overrideToolchain rust-toolchain;

        ytsub = craneLib.buildPackage {
          CARGO_PROFILE = "release-lto";

          src = craneLib.cleanCargoSource ./.;

          buildInputs = [
            pkgs.sqlite.dev
          ];
        };
      in
      {
        packages.default = ytsub;

        devShells.default = craneLib.devShell {
          inputsFrom = [ ytsub ];

          packages = with pkgs; [
            cargo-edit
          ];
        };
      }
    );
}
