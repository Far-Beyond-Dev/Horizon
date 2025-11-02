{
  description = "Horizon Rust Game Server - Nix Flake";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs { inherit system; };
      in {
        devShell = pkgs.mkShell {
          buildInputs = [
            pkgs.rustc
            pkgs.cargo
            pkgs.pkg-config
            pkgs.openssl
            pkgs.protobuf
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
