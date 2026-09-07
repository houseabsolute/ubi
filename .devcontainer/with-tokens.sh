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

set -euo pipefail

for pair in github:GITHUB_TOKEN gitlab:GITLAB_TOKEN codeberg:CODEBERG_TOKEN; do
    forge=${pair%%:*}
    var=${pair##*:}

    value=$(git config "$forge.ubiTestingToken" 2>/dev/null || true)
    # Inside the dev container there is no global git config to read, but devcontainer.json's
    # remoteEnv has already forwarded the token from the host, so keep what's already there.
    if [ -z "$value" ]; then
        value=${!var:-}
    fi

    if [ -z "$value" ]; then
        echo "Warning: no $var - set $forge.ubiTestingToken in your git config, or the tests" \
            "will run unauthenticated and may fail against the forge's rate limits." >&2
        continue
    fi

    export "$var=$value"
done

exec "$@"
