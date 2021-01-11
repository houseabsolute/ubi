#!/bin/sh

set -e

TAG=$(
    # From https://gist.github.com/lukechilds/a83e1d7127b78fef38c2914c4ececc3c
    curl --silent "https://api.github.com/repos/houseabsolute/ubi/releases/latest" |
        grep '"tag_name":' |
        sed -E 's/.*"([^"]+)".*/\1/'
)

if [ -z "$TAG" ]; then
    echo "Cannot find a UBI release!"
    exit 1
fi

TARGET="$HOME/bin"
if [ $(id -u) -eq 0 ]; then
    TARGET="/usr/local/bin"
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
        echo "Cannot determine what binary to download for your kernel: $KERNEL"
        exit 2
        ;;
esac

ARCH=$(uname -p)
case "$ARCH" in
     x86_64)
         CPU="x86_64"
         ;;
     *)
         echo "Cannot determine what binary to download for your CPU architecture: $ARCH"
         exit 3
esac

curl --silent --location https://github.com/houseabsolute/ubi/releases/download/"$TAG"/ubi-"$PLATFORM"-"$CPU".tar.gz |
    tar -xzf - ubi 
