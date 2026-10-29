{ pkgs, lib, ... }:

let
  # Native libraries Bevy needs at build and/or run time on Linux.
  runtimeLibs = with pkgs; [
    alsa-lib
    udev
    vulkan-loader
    wayland
    libxkbcommon
    xorg.libX11
    xorg.libXcursor
    xorg.libXi
    xorg.libXrandr
  ];
in
{
  languages.rust = {
    enable = true;
    channel = "stable";
    components = [
      "rustc"
      "cargo"
      "clippy"
      "rustfmt"
      "rust-analyzer"
      "rust-src"
      "llvm-tools-preview"
    ];
  };

  packages =
    with pkgs;
    [
      pkg-config
      cargo-deny
      # Upstream test suite fails in the nix sandbox; the tool itself works.
      (cargo-llvm-cov.overrideAttrs { doCheck = false; })
      cargo-outdated
      cargo-machete
      actionlint
      just
    ]
    ++ runtimeLibs;

  env.LD_LIBRARY_PATH = lib.makeLibraryPath runtimeLibs;

  enterShell = ''
    echo "Prismapsis dev shell: $(rustc --version)"
    just --list
  '';

  enterTest = ''
    just ci
  '';
}
