#!/bin/sh

set -e

# We need a space in the Authorization header, which means we need to quote it
# in the curl command later, as "$AUTH". That means we can't put the
# `--header` flag in this variable, because if we do then the entire string of
# `--header Authorization:token ...` is interpreted as a single unit by the
# shell, and curl is confused. So we need to hardcode the `--header` in the
# command later. But that means we have to have _something_ to put after
# `--header`, even if it's nonsense.
echo ""
if [ -n "$GITHUB_TOKEN" ]; then
    echo "Setting Authorization header using GITHUB_TOKEN env var"
    AUTH="Authorization:token $GITHUB_TOKEN"
else
    echo "GITHUB_TOKEN env var is not set"
    AUTH="X-Cannot-Be-Empty"
fi

if [ -z "$TAG" ]; then
    URI="https://api.github.com/repos/houseabsolute/ubi/releases/latest"
    RELEASES=$(curl --header "$AUTH" --show-error --silent $URI)
    if [ -z "$RELEASES" ]; then
        >&2 echo "Did not get a response body back from $URI"
        exit 1
    fi

    TAG=$(
        # From https://gist.github.com/lukechilds/a83e1d7127b78fef38c2914c4ececc3c
        echo "$RELEASES" |
            grep '"tag_name":' |
            sed -E 's/.*"([^"]+)".*/\1/'
    )

    if [ -z "$TAG" ]; then
        >&2 echo "boostrap-ubi.sh: Cannot find a UBI release based on GitHub API response!"
        >&2 echo ""
        >&2 echo "$RELEASES"
        exit 2
    fi
fi

if [ $(id -u) -eq 0 ]; then
    DEFAULT_TARGET="/usr/local/bin"
else
    DEFAULT_TARGET="$HOME/bin"
fi

TARGET="${TARGET:=$DEFAULT_TARGET}"

if [ ! -d "$TARGET" ]; then
    >&2 echo "boostrap-ubi.sh: The install target directory, $TARGET, does not exist"
    exit 3
fi

cd "$TARGET"

KERNEL=$(uname -s)
ABI=""
EXT="tar.gz"
case "$KERNEL" in
    Linux)
        PLATFORM="Linux"
        ABI="-musl"
        ;;
    Darwin)
        PLATFORM="Darwin"
        ;;
    MINGW*)
        PLATFORM="Windows"
        EXT="zip"
        ;;
    *)
        echo "boostrap-ubi.sh: Cannot determine what binary to download for your kernel: $KERNEL"
        exit 3
        ;;
esac

# I previous had uname -p but that reports all sorts of weird stuff. On one
# person's Linux x86_64 machine it reported "unknown". On macOS x86_64 you get
# "i386". Why? I have no idea.
ARCH=$(uname -m)
case "$ARCH" in
    x86_64)
        CPU="x86_64"
        ;;
    arm64)
        CPU="aarch64"
        ;;
    *)
        echo "boostrap-ubi.sh: Cannot determine what binary to download for your CPU architecture: $ARCH"
        exit 4
esac

FILENAME="ubi-$PLATFORM-$CPU$ABI.$EXT"
URL="https://github.com/houseabsolute/ubi/releases/download/$TAG/$FILENAME"

TEMPDIR=$( mktemp -d )
trap 'rm -rf -- "$TEMPDIR"' EXIT
LOCAL_FILE="$TEMPDIR/$FILENAME"

echo "downloading $URL"
STATUS=$( curl --silent --output "$LOCAL_FILE" --write-out "%{http_code}" --location "$URL" )
if [ -z "$STATUS" ]; then
    >&2 echo "curl failed to download $URL and did not print a status code"
    exit 5
elif [ "$STATUS" != "200" ]; then
    >&2 echo "curl failed to download $URL with status code = $STATUS"
    exit 6
fi

if [ "$EXT" = "tar.gz" ]; then
    tar -xzf "$LOCAL_FILE" ubi
else
    unzip "$LOCAL_FILE"
fi

rm -rf -- "$TEMPDIR"

echo ""
echo "boostrap-ubi.sh: ubi has been installed to $TARGET."

set +e
echo ":$PATH:" | grep --extended-regexp ":$TARGET:" > /dev/null
if [ "$?" != "0" ]; then
    echo "boostrap-ubi.sh: It looks like $TARGET is not in your PATH. You may want to add it to use ubi."
fi

echo ""
