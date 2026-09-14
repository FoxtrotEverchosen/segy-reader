{ pkgs ? import <nixpkgs> {} }:
pkgs.mkShell {
  buildInputs = [
    pkgs.python3
    pkgs.maturin
    pkgs.cargo
    pkgs.rustc
  ] ++ (with pkgs.python3Packages; [
    pip
    matplotlib
    numpy
    pyqt6
    scipy
    pytest
  ]);

  shellHook = ''
    if [ ! -d .venv ] || ! .venv/bin/python --version &>/dev/null; then
      echo "Recreating .venv..."
      rm -rf .venv
      python -m venv .venv
    fi
    source .venv/bin/activate
  '';
}
