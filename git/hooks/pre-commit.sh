#!/bin/bash

set -e

# Hooks run in a non-interactive shell, where mise's shell activation has not happened, and both
# `just` and the tools it drives are mise-managed. Some contexts that run hooks (GUI git clients,
# editors) don't even have mise's own install dir on PATH, so find it and put it there. The
# Justfile shells out to .devcontainer/up.sh, which calls mise too, hence exporting rather than
# just calling it by path.
mise=$(command -v mise || true)
if [ -z "$mise" ] && [ -x "$HOME/.local/bin/mise" ]; then
    mise="$HOME/.local/bin/mise"
    export PATH="$HOME/.local/bin:$PATH"
fi
if [ -z "$mise" ]; then
    echo "Cannot find mise, which is needed to run the linters. See https://mise.jdx.dev/." >&2
    exit 1
fi

exec "$mise" exec -- just lint -s
