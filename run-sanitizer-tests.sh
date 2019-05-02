#!/usr/bin/env bash

export RUST_TEST_THREADS=1

RUSTFLAGS="-Z sanitizer=address" cargo +nightly test sanitizer_tests --target x86_64-unknown-linux-gnu -- --ignored
RUSTFLAGS="-Z sanitizer=leak" cargo +nightly test sanitizer_tests --target x86_64-unknown-linux-gnu -- --ignored
RUSTFLAGS="-Z sanitizer=thread" cargo +nightly test sanitizer_tests --target x86_64-unknown-linux-gnu -- --ignored

# MemSan is currently broken. See:
# https://github.com/japaric/rust-san/blob/master/README.md#memorysanitizer-use-of-uninitialized-value-in-the-test-runner
#RUSTFLAGS="-Z sanitizer=memory" xargo nightly test --target x86_64-unknown-linux-gnu