{ pkgs, lib, ... }:

let
  # Native libraries Bevy needs at build and/or run time on Linux.
  # wasm-bindgen-cli at exactly the version of the wasm-bindgen crate in
  # Cargo.lock, built from crates.io, as nixpkgs may lag behind it.
  wasmBindgenCli = pkgs.buildWasmBindgenCli rec {
    src = pkgs.fetchCrate {
      pname = "wasm-bindgen-cli";
      version = "0.2.130";
      hash = "sha256-9Og1O/6/VC3sc5hBjoIWBErmLW2UHGMsTQO+6+m/dng=";
    };
    cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
      inherit src;
      inherit (src) pname version;
      hash = "sha256-j81sUf1m5yXU4FJfGj7GV+hdqVCV8mRmNb0QIsMCjqw=";
    };
  };

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
      wasmBindgenCli
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
