{ pkgs ? import <nixpkgs> {} }:
pkgs.mkShell {
  buildInputs = [
    pkgs.python3
    pkgs.maturin
  ] ++ (with pkgs.python3Packages; [
    pip
    matplotlib
    numpy
    pyqt6
    scipy
    pytest
  ]);

  shellHook = ''
    if [ ! -d .venv ]; then
      python -m venv .venv
    fi
    source .venv/bin/activate
  '';
}
