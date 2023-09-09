#!/bin/bash

cargo build --release && docker build -t ambrogio:latest .
