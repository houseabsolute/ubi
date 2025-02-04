#!/bin/sh

if [ -n "$UBI_DEBUG_BOOTSTRAP" ]; then
    set -x
fi

set -e

if [ "$(id -u)" -eq 0 ]; then
    DEFAULT_TARGET="/usr/local/bin"
else
    DEFAULT_TARGET="$HOME/bin"
fi

TARGET="${TARGET:=$DEFAULT_TARGET}"

if [ ! -d "$TARGET" ]; then
    >&2 echo "bootstrap-ubi.sh: The install target directory, \`$TARGET\`, does not exist"
    exit 3
fi

# Old vs new file naming. This changes in v0.2.1 when I started using actions-rust-release to do
# releases.
#
# v0.1.1/ubi-FreeBSD-x86_64.tar.gz
# v0.2.1/ubi-FreeBSD-x86_64.tar.gz
#
# v0.1.1/ubi-Linux-powerpc-gnu.tar.gz
# v0.2.1/ubi-Linux-gnu-powerpc.tar.gz
#
# v0.1.1/ubi-Linux-powerpc64-gnu.tar.gz
# v0.2.1/ubi-Linux-gnu-powerpc64.tar.gz
#
# v0.1.1/ubi-Linux-powerpc64le.tar.gz
# v0.2.1/ubi-Linux-gnu-powerpc64le.tar.gz
#
# v0.1.1/ubi-Linux-riscv64gc-gnu.tar.gz
# v0.2.1/ubi-Linux-gnu-riscv64gc.tar.gz
#
# v0.1.1/ubi-Linux-s390x-gnu.tar.gz
# v0.2.1/ubi-Linux-gnu-s390x.tar.gz
#
# v0.1.1/ubi-Linux-aarch64-musl.tar.gz
# v0.2.1/ubi-Linux-musl-arm64.tar.gz
#
# v0.1.1/ubi-Linux-i686-musl.tar.gz
# v0.2.1/ubi-Linux-musl-i686.tar.gz
#
# v0.1.1/ubi-Linux-x86_64-musl.tar.gz
# v0.2.1/ubi-Linux-musl-x86_64.tar.gz
#
# v0.1.1/ubi-Linux-arm-musl.tar.gz
# v0.2.1/ubi-Linux-musleabi-arm.tar.gz
#
# v0.1.1/ubi-Darwin-aarch64.tar.gz
# v0.2.1/ubi-macOS-arm64.tar.gz
#
# v0.1.1/ubi-Darwin-x86_64.tar.gz
# v0.2.1/ubi-macOS-x86_64.tar.gz
#
# v0.1.1/ubi-NetBSD-x86_64.tar.gz
# v0.2.1/ubi-NetBSD-x86_64.tar.gz
#
# v0.1.1/ubi-Windows-aarch64.zip
# v0.2.1/ubi-Windows-msvc-arm64.zip
#
# v0.1.1/ubi-Windows-i686.zip
# v0.2.1/ubi-Windows-msvc-i686.zip
#
# v0.1.1/ubi-Windows-x86_64.zip
# v0.2.1/ubi-Windows-msvc-x86_64.zip

OLD_FILE_NAMING=""
if [ -n "$TAG" ]; then
    IFS="." read -r MAJOR MINOR <<EOF
$TAG
EOF
    if [ "$MAJOR" = "v0" ] && [ "$MINOR" -lt 2 ]; then
        OLD_FILE_NAMING="true"
    fi
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
        if [ -n "$OLD_FILE_NAMING" ]; then
            OS="Darwin"
        else
            OS="macOS"
        fi
        ;;
    FreeBSD)
        OS="FreeBSD"
        ;;
    NetBSD)
        OS="NetBSD"
        ;;
    MINGW*)
        OS="Windows"
        # Only 0.2.1+ include the libc in Windows filenames.
        if [ -z "$OLD_FILE_NAMING" ]; then
            LIBC="-msvc"
        fi
        EXT="zip"
        ;;
    *)
        echo "bootstrap-ubi.sh: Cannot determine what binary to download for your kernel: $KERNEL"
        exit 3
        ;;
    esac

    # I previously had `uname -p` but that reports all sorts of weird stuff. On one
    # person's Linux x86_64 machine it reported "unknown". On macOS x86_64 you get
    # "i386". Why? I have no idea.
    ARCH=$(uname -m)
    case "$ARCH" in
    i386 | i486 | i586 | i686)
        CPU="i686"
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
            if [ -n "$OLD_FILE_NAMING" ]; then
                LIBC="-musl"
            else
                LIBC="-musleabi"
            fi
        fi
        ;;
    aarch64 | arm64)
        if [ -n "$OLD_FILE_NAMING" ]; then
            CPU="aarch64"
        else
            CPU="arm64"
        fi
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
        if [ "$OS" = "Linux" ]; then
            LIBC="-gnu"
        fi
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
        echo "bootstrap-ubi.sh: Cannot determine what binary to download for your CPU architecture: $ARCH"
        exit 4
        ;;
    esac

    if [ -n "$OLD_FILE_NAMING" ]; then
        FILENAME="ubi-$OS-$CPU$LIBC.$EXT"
    else
        FILENAME="ubi-$OS$LIBC-$CPU.$EXT"
    fi
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
STATUS=$(curl --silent --show-error --location --output "$LOCAL_FILE" --write-out "%{http_code}" "$URL")
if [ -z "$STATUS" ]; then
    >&2 echo "curl failed to download $URL and did not print a status code"
    exit 5
elif [ "$STATUS" != "200" ]; then
    >&2 echo "curl failed to download $URL - status code = $STATUS"
    exit 6
fi

if echo "$FILENAME" | grep "\\.tar\\.gz$"; then
    tar -xzf "$LOCAL_FILE" ubi
else
    unzip "$LOCAL_FILE"
fi

chmod +x ubi

rm -rf -- "$TEMPDIR"

echo ""
echo "bootstrap-ubi.sh: ubi has been installed to \`$TARGET\`."

set +e
TARGET_IS_IN_PATH=$(echo ":$PATH:" | grep --extended-regexp ":$TARGET:" 2>/dev/null)
if [ -z "$TARGET_IS_IN_PATH" ]; then
    echo "bootstrap-ubi.sh: It looks like \`$TARGET\` is not in your PATH. You may want to add it to use ubi."
fi

echo ""
