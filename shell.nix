{ pkgs ? import <nixpkgs> {} }:

# Pin rust version?
pkgs.mkShell {
  nativeBuildInputs = with pkgs; [
    # For building
    pkgconfig
    # Dependencies
    openssl
  ];
}
