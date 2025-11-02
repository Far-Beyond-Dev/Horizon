{ pkgs ? import <nixpkgs> {} }:

pkgs.mkShell {
  buildInputs = [
    pkgs.rustc
    pkgs.cargo
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
}
