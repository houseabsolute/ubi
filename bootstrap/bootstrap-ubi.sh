#!/bin/sh

set -e

if [ -n "$GITHUB_TOKEN" ]; then
    AUTH="--header Authorization:token%20$GITHUB_TOKEN"
fi

URI="https://api.github.com/repos/houseabsolute/ubi/releases/latest"
RELEASES=$(curl $AUTH --show-error --silent $URI)
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

TARGET="${TARGET:=$HOME/bin}"

if [ $(id -u) -eq 0 ]; then
    TARGET="/usr/local/bin"
fi

if [ ! -d "$TARGET" ]; then
    2>& echo "boostrap-ubi.sh: The install target directory, $TARGET, does not exist"
    exit 3
fi

cd "$TARGET"

KERNEL=$(uname -s)
case "$KERNEL" in
    Linux)
        PLATFORM="Linux"
        ;;
    Darwin)
        PLATFORM="Darwin"
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
    *)
        echo "boostrap-ubi.sh: Cannot determine what binary to download for your CPU architecture: $ARCH"
        exit 4
esac

curl --silent --location https://github.com/houseabsolute/ubi/releases/download/"$TAG"/ubi-"$PLATFORM"-"$CPU".tar.gz |
    tar -xzf - ubi 

echo ""
echo "boostrap-ubi.sh: ubi has been installed to $TARGET."

set +e
echo ":$PATH:" | grep --extended-regexp ":$TARGET:" > /dev/null
if [ "$?" != "0" ]; then
    echo "boostrap-ubi.sh: It looks like $TARGET is not in your PATH. You may want to add it to use ubi."
fi

echo ""
