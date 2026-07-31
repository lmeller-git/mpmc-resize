default:
    @just --list

test-rs:
    cargo test --locked --all-features --all-targets
    cargo test --locked --all-features --doc
    cargo test --locked --no-default-features --all-targets
    cargo test --locked --no-default-features --doc
    cargo +nightly miri test --locked --all-features
    LOOM_MAX_PREEMPTIONS=2 RUSTFLAGS="--cfg loom" cargo test --locked --lib --all-features --release
    RUSTFLAGS="--cfg shuttle" cargo test --locked --lib --all-features
    RUSTFLAGS="--cfg echeneis" cargo test --locked --lib --all-features

lint-rs:
    cargo +nightly fmt --all -- --check
    cargo clippy --all-targets -- -D warnings

check-rs:
    cargo +nightly docs-rs
    cargo hack --feature-powerset check
    cargo semver-checks --all-features

test: test-rs

lint: lint-rs

check: check-rs

bench:
    cargo bench --all-features

