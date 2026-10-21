#!/bin/bash
#
# Run a command with the integration test tokens in its environment. Without them the tests run
# unauthenticated and hit the forges' rate limits.
#
# These are set here rather than exported from the Justfile because just's `export` is global,
# which would hand the testing tokens to every recipe, including `release`, where cargo-release
# talks to GitHub as the user and should use the user's own credentials.
#
# Passing them through the environment rather than on the command line also keeps them out of `ps`
# output.
#
# This runs in two places. On the host, it wraps `devcontainer exec` and exports each token as
# UBI_FORWARDED_<NAME>, which devcontainer.json's remoteEnv forwards into the container. Inside the
# container, it wraps the actual command and copies the forwarded values to their real names.
#
# The forwarded names exist because remoteEnv sets every variable it lists, using an empty string
# when the host has no value. If it listed GITHUB_TOKEN directly, every command in the container
# would get an empty GITHUB_TOKEN, and some tools, like zizmor, refuse to run with an empty token
# rather than treating it as unset.

set -euo pipefail

for pair in github:GITHUB_TOKEN gitlab:GITLAB_TOKEN codeberg:CODEBERG_TOKEN; do
    forge=${pair%%:*}
    var=${pair##*:}
    forwarded="UBI_FORWARDED_$var"

    if [ -n "${UBI_DEVCONTAINER:-}" ]; then
        value=${!forwarded:-}
    else
        value=$(git config "$forge.ubiTestingToken" 2>/dev/null || true)
    fi
    # Fall back to a token that's already in the environment, like one the user exported on the
    # host, or one set by an earlier run of this script in a `just shell` session.
    if [ -z "$value" ]; then
        value=${!var:-}
    fi

    if [ -z "$value" ]; then
        # The host side is always followed by the container side, so only warn once, from the
        # place where the command actually runs.
        if [ -n "${UBI_DEVCONTAINER:-}" ]; then
            echo "Warning: no $var - set $forge.ubiTestingToken in your git config, or the tests" \
                "will run unauthenticated and may fail against the forge's rate limits." >&2
        fi
        continue
    fi

    if [ -n "${UBI_DEVCONTAINER:-}" ]; then
        export "$var=$value"
    else
        export "$forwarded=$value"
    fi
done

exec "$@"
