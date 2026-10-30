# Common development tasks. Run inside the devenv shell (`devenv shell` or direnv).

coverage_args := "--all-features --locked --ignore-filename-regex 'src/main.rs' --fail-under-lines 80"

# List available recipes
default:
    @just --list

# Run the game with a dev build
run *args:
    cargo run -- {{args}}

# Run the game with an optimised release build
release *args:
    cargo run --release -- {{args}}

# Format all code
fmt:
    cargo fmt --all

# Check formatting and run clippy
lint:
    cargo fmt --all -- --check
    cargo clippy --all-targets --all-features --locked -- -D warnings

# Run the tests
test *args:
    cargo test --all-features --locked {{args}}

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
