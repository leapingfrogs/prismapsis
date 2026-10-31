# Common development tasks. Run inside the devenv shell (`devenv shell` or direnv).

# main.rs and the dev-only UI glue can't run headless, so are excluded.
coverage_args := "--workspace --all-features --locked --ignore-filename-regex 'src/(main|dev_tools)\\.rs' --fail-under-lines 80"

# List available recipes
default:
    @just --list

# Run the game with a dev build and the tuning panel
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

# Regenerate the sound effects in assets/audio from tools/sfx
sfx:
    cargo run --locked -p sfx

# Run the game with an optimised release build
release *args:
    cargo run --release -- {{args}}

# Format all code
fmt:
    cargo fmt --all

# Check formatting and run clippy
lint:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --all-features --locked -- -D warnings

# Run the tests
test *args:
    cargo test --workspace --all-features --locked {{args}}

# Run tests with coverage, failing below the threshold
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

# Run every check CI runs (outdated is informational and excluded)
ci: lint lint-commits coverage audit

# Remove build artifacts
clean:
    cargo clean
