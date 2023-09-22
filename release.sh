#!/bin/bash

# Do the tets
cargo test

# Set version in cargo.toml

# Build and push docker image
# Since our package knows excactly how to build itself and manages its own versioning:
cargo run -- docker --use-package-version up 

# Create a ready to use crd.yaml
cargo run -- crd write -f crd.yaml

# Create a ready to use operator manifest
cargo run -- operator controller deploy write -f operator.yaml