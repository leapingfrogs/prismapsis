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
    # The web build.
    targets = [ "wasm32-unknown-unknown" ];
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
      committed
      just
      # The web build: wasm-bindgen-cli must match the wasm-bindgen crate
      # in Cargo.lock; binaryen provides wasm-opt.
      wasm-bindgen-cli
      binaryen
      miniserve
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
