#!/bin/sh

set -e

if [ "$(id -u)" -eq 0 ]; then
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

if [ -z "$FILENAME" ]; then
    KERNEL=$(uname -s)
    LIBC=""
    EXT="tar.gz"
    case "$KERNEL" in
    Linux)
        OS="Linux"
        ;;
    Darwin)
        OS="Darwin"
        ;;
    FreeBSD)
        OS="FreeBSD"
        ;;
    NetBSD)
        OS="NetBSD"
        ;;
    MINGW*)
        OS="Windows"
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
    i386 | i486 | i586 | i686)
        CPU="i786"
        if [ "$OS" = "Linux" ]; then
            LIBC="-musl"
        fi
        ;;
    x86_64 | amd64)
        CPU="x86_64"
        if [ "$OS" = "Linux" ]; then
            LIBC="-musl"
        fi
        ;;
    arm | armv5* | armv6* | armv7*)
        CPU="arm"
        if [ "$OS" = "Linux" ]; then
            LIBC="-musl"
        fi
        ;;
    aarch64 | arm64)
        CPU="aarch64"
        if [ "$OS" = "Linux" ]; then
            LIBC="-musl"
        fi
        ;;
    mips)
        CPU="mips"
        ;;
    mipsel | mipsle)
        CPU="mipsel"
        ;;
    mips64)
        CPU="mips64"
        ;;
    mips64el | mips64le)
        CPU="mips64el"
        ;;
    powerpc | ppc)
        CPU="powerpc"
        if [ "$OS" = "Linux" ]; then
            LIBC="-gnu"
        fi
        ;;
    powerpc64 | ppc64)
        CPU="powerpc64"
        if [ "$OS" = "Linux" ]; then
            LIBC="-gnu"
        fi
        ;;
    powerpc64le | ppc64le)
        CPU="powerpc64le"
        ;;
    riscv64 | rv64gc)
        CPU="riscv64gc"
        if [ "$OS" = "Linux" ]; then
            LIBC="-gnu"
        fi
        ;;
    s390x)
        CPU="s390x"
        if [ "$OS" = "Linux" ]; then
            LIBC="-gnu"
        fi
        ;;
    *)
        echo "boostrap-ubi.sh: Cannot determine what binary to download for your CPU architecture: $ARCH"
        exit 4
        ;;
    esac

    FILENAME="ubi-$OS-$CPU$LIBC.$EXT"
fi

if [ -z "$TAG" ]; then
    URL="https://github.com/houseabsolute/ubi/releases/latest/download/$FILENAME"
else
    URL="https://github.com/houseabsolute/ubi/releases/download/$TAG/$FILENAME"
fi

TEMPDIR=$(mktemp -d)
trap 'rm -rf -- "$TEMPDIR"' EXIT
LOCAL_FILE="$TEMPDIR/$FILENAME"

echo "downloading $URL"
STATUS=$(curl --silent --output "$LOCAL_FILE" --write-out "%{http_code}" --location "$URL")
if [ -z "$STATUS" ]; then
    >&2 echo "curl failed to download $URL and did not print a status code"
    exit 5
elif [ "$STATUS" != "200" ]; then
    >&2 echo "curl failed to download $URL with status code = $STATUS"
    exit 6
fi

if echo "$FILENAME" | grep "\\.tar\\.gz$"; then
    tar -xzf "$LOCAL_FILE" ubi
else
    unzip "$LOCAL_FILE"
fi

rm -rf -- "$TEMPDIR"

echo ""
echo "boostrap-ubi.sh: ubi has been installed to $TARGET."

set +e
TARGET_IS_IN_PATH=$(echo ":$PATH:" | grep --extended-regexp ":$TARGET:" 2>/dev/null)
if [ -z "$TARGET_IS_IN_PATH" ]; then
    echo "boostrap-ubi.sh: It looks like $TARGET is not in your PATH. You may want to add it to use ubi."
fi

echo ""
