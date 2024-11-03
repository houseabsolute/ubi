#!/bin/sh

set -e
set -x

SCRIPT="$1"
# shellcheck disable=SC2034 # This is used by the bootstrap scripts.
TAG="$2"

mkdir -p "$HOME/bin"

UBI_DEBUG_BOOTSTRAP=1 "./bootstrap/$SCRIPT"
if [ ! -x "$HOME/bin/ubi" ]; then
    echo "Running ./bootstrap/$SCRIPT did not install ubi!"
    exit 1
fi

"$HOME/bin/ubi" --project houseabsolute/precious --in "$HOME/bin"
if [ ! -x "$HOME/bin/precious" ]; then
    echo "Running ubi did not install precious!"
    exit 1
fi

exit 0
