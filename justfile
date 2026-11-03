# Common development tasks. Run inside the devenv shell (`devenv shell` or direnv).

# main.rs, the dev-only UI glue and the command-line tools in src/bin are
# thin wrappers that tests can't drive, so are excluded.
coverage_args := "--workspace --all-features --locked --ignore-filename-regex 'src/(main|dev_tools)\\.rs|src/bin/' --fail-under-lines 80"

# List available recipes
default:
    @just --list

# Run the game with a dev build and the tuning panel. Play a level file
# with `just run --level assets/levels/tower.toml`; saving it reloads it.
run *args:
    cargo run --features dev -- {{args}}

# Run under gamescope with the MangoHud overlay, for steady frame timing.
# WAYLAND_DISPLAY is unset so the game uses gamescope's X server rather than
# talking to the host compositor; gamescope itself runs without the dev
# shell's library path. Extra gamescope flags go in GAMESCOPE_ARGS; set the
# game size (-w/-h) as well as the output size (-W/-H), or gamescope renders
# at 1280x720 and upscales, e.g.:
#   GAMESCOPE_ARGS="-f -W 3840 -H 2160 -w 3840 -h 2160 -r 60" just run-gamescope
# Set MANGOAPP=1 to add the MangoHud overlay.
run-gamescope *args:
    cargo build --features dev
    env -u WAYLAND_DISPLAY -u LD_LIBRARY_PATH gamescope ${MANGOAPP:+--mangoapp} ${GAMESCOPE_ARGS:-} -- env LD_LIBRARY_PATH="$LD_LIBRARY_PATH" target/debug/prismapsis --fullscreen {{args}}

# Check level files for problems and that each can be finished (every
# level by default)
level-check *files:
    cargo run --locked --bin levels -- check {{files}}

# Remake generated levels from their [generated] settings; --check only
# reports files that are out of date
levelgen *args:
    cargo run --locked --bin levels -- generate {{args}}

# Print level maps in colour (every level by default; the campaign file
# shows its levels in play order)
level-show *files:
    cargo run --locked --bin levels -- show {{files}}

# Paint the themed tilesets and the sprite sheet in assets/art; `--check`
# reports images that are out of date, and `preview DIR` draws a sample of
# each theme and the sprites in their colours into DIR
art *args:
    cargo run --locked --bin art -- {{args}}

# Regenerate the sound effects and music in assets/audio from tools/sfx
sfx:
    cargo run --locked -p sfx

# Run the game with an optimised release build
release *args:
    cargo run --release -- {{args}}

# The WebAssembly features wasm-opt may use: those rustc's wasm32 target
# enables by default, which every current browser supports.
wasm_features := "--enable-bulk-memory --enable-multivalue --enable-mutable-globals --enable-nontrapping-float-to-int --enable-reference-types --enable-sign-ext"

# Build the web version (WebGL2) into dist/web, optimised for size
web:
    cargo build --profile web --locked --target wasm32-unknown-unknown --bin prismapsis
    rm -rf dist/web
    wasm-bindgen --target web --no-typescript --out-dir dist/web --out-name prismapsis target/wasm32-unknown-unknown/web/prismapsis.wasm
    wasm-opt -Oz {{wasm_features}} --strip-debug --strip-producers -o dist/web/prismapsis_bg.wasm dist/web/prismapsis_bg.wasm
    cp web/index.html web/manifest.webmanifest assets/icon/tile_*.png assets/icon/rounded_32.png dist/web/
    cp LICENSE-MIT LICENSE-APACHE assets/fonts/Exo2-OFL.txt dist/web/

# Build the web version and serve it at http://localhost:8765 (or another
# port)
web-serve port="8765": web
    miniserve --index index.html --port {{port}} dist/web

# Format all code
fmt:
    cargo fmt --all

# Check formatting and run clippy, for the desktop and the web
lint:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
    cargo clippy --locked --target wasm32-unknown-unknown --lib --bin prismapsis -- -D warnings

# Run the tests
test *args:
    cargo test --workspace --all-features --locked {{args}}

# Run tests with coverage, failing below the threshold. The slowest tests are
# left out, as coverage makes them about 20 times slower; `just test` runs them
coverage:
    cargo llvm-cov {{coverage_args}}

# Generate an HTML coverage report and open it
coverage-html:
    cargo llvm-cov {{coverage_args}} --html --open

# Security advisories, licences, sources and unused dependencies
audit:
    cargo deny --locked check
    cargo machete

# Report outdated direct dependencies
outdated:
    cargo outdated --root-deps-only --exit-code 1

# Check commit messages not yet on origin/main follow Conventional Commits
lint-commits:
    committed -vv --no-merge-commit origin/main..HEAD

# Run every check CI runs (outdated is informational and excluded); `test`
# adds the tests too slow to run under coverage
ci: lint lint-commits coverage test audit

# Remove build artifacts
clean:
    cargo clean
