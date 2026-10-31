#!/bin/bash

ENGINE=$(command -v docker || command -v podman) || { echo "neither docker nor podman found" >&2; exit 1; }

cargo build --release && "$ENGINE" build -t ambrogio:latest .
