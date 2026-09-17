{ pkgs ? import <nixpkgs> {} }:
pkgs.mkShell {
  buildInputs = [
    pkgs.python3
    pkgs.maturin
    pkgs.cargo
    pkgs.rustc
    pkgs.clippy
    pkgs.rust-analyzer
    pkgs.rustfmt
  ] ++ (with pkgs.python3Packages; [
    pip
    matplotlib
    numpy
    pyqt6
    scipy
    pytest
  ]);

  NIX_ENFORCE_PURITY = "0";
  RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";

  shellHook = ''
    if [ ! -d .venv ] || ! .venv/bin/python --version &>/dev/null; then
      echo "Recreating .venv..."
      rm -rf .venv
      python -m venv .venv --system-site-packages
    fi
    source .venv/bin/activate
    maturin develop --release
  '';
}
