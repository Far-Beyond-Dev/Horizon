{
  description = "Horizon Rust Game Server - Nix Flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-23.11";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
        rustToolchain = pkgs.rust-bin.stable.latest.default;
      in {
        devShell = pkgs.mkShell {
          buildInputs = [
            rustToolchain
            pkgs.pkg-config
            pkgs.openssl
            pkgs.protobuf
            pkgs.libssl
          ];
          shellHook = ''
            export OPENSSL_DIR=${pkgs.openssl.dev}
            export PKG_CONFIG_PATH=${pkgs.openssl.dev}/lib/pkgconfig
            export PATH=${pkgs.protobuf}/bin:$PATH
            echo "Horizon dev shell loaded!"
          '';
        };
      });
}
