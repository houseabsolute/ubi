#!/bin/sh

set -e

if [ -n "$GITHUB_AUTH" ]; then
    USER="--user $GITHUB_AUTH"
fi

TAG=$(
    # From https://gist.github.com/lukechilds/a83e1d7127b78fef38c2914c4ececc3c
    curl --silent $USER "https://api.github.com/repos/houseabsolute/ubi/releases/latest" |
        grep '"tag_name":' |
        sed -E 's/.*"([^"]+)".*/\1/'
)

if [ -z "$TAG" ]; then
    echo "boostrap-ubi.sh: Cannot find a UBI release!"
    exit 1
fi

TARGET="$HOME/bin"
if [ $(id -u) -eq 0 ]; then
    TARGET="/usr/local/bin"
fi

if [ ! -d "$TARGET" ]; then
    echo "boostrap-ubi.sh: The install target directory, $TARGET, does not exist"
    exit 2
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
