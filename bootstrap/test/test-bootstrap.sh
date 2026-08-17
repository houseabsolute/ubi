#!/bin/sh

set -e
set -x

SCRIPT="$1"
# The bootstrap scripts read this from the environment, so it has to be
# exported, not just set.
export TAG="$2"

mkdir -p "$HOME/bin"

UBI_DEBUG_BOOTSTRAP=1 "./bootstrap/$SCRIPT"
if [ ! -x "$HOME/bin/ubi" ]; then
    echo "Running ./bootstrap/$SCRIPT did not install ubi!"
    exit 1
fi

if [ -n "$TAG" ]; then
    # When a tag is requested, all we care about is that the bootstrap script
    # resolved it to the right release. Old versions of ubi cannot necessarily
    # install anything on a current platform - 0.0.15 looks for "Darwin" and
    # "aarch64" in release filenames, for example - so we do not ask them to.
    WANT="${TAG#v}"
    GOT=$("$HOME/bin/ubi" --version | awk '{ print $NF }')
    if [ "$GOT" != "$WANT" ]; then
        echo "Running ./bootstrap/$SCRIPT with TAG=$TAG installed ubi $GOT!"
        exit 1
    fi

    exit 0
fi

"$HOME/bin/ubi" --project houseabsolute/precious --in "$HOME/bin"
if [ ! -x "$HOME/bin/precious" ]; then
    echo "Running ubi did not install precious!"
    exit 1
fi

exit 0
