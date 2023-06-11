#!/bin/bash

cargo build --release && docker build -t ambrogio:1.0.0 .
