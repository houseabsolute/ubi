#!/bin/bash

cargo release --package ubi "$@"
cargo release --package ubi-cli "$@"
