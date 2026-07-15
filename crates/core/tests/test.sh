#!/bin/bash
RUST_TEST_THREADS=1 cargo test --package core --test integration --  --show-output