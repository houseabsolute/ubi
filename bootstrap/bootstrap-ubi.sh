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
#
# Some v0.0.x releases on Linux don't fit the old naming either:
#
# v0.0.10/ubi-Linux-x86_64.tar.gz (no libc before v0.0.11)
# v0.0.10/ubi-Linux-armv7.tar.gz
# v0.0.23/ubi-Linux-i586-musl.tar.gz (i586 from v0.0.20 to v0.0.23)

OLD_FILE_NAMING=""
V0_0_PATCH=""
if [ -n "$TAG" ]; then
    # Note that we cannot use `read` to split the tag on dots here. With a tag like `v0.0.15`,
    # `read -r MAJOR MINOR` assigns the entire rest of the tag, `0.15`, to `$MINOR`, which is not a
    # number. Comparing that with `-lt` is a fatal error under some shells, notably busybox `sh`.
    MAJOR="${TAG%%.*}"
    MINOR_AND_REST="${TAG#*.}"
    MINOR="${MINOR_AND_REST%%.*}"
    if [ "$MAJOR" = "v0" ]; then
        case "$MINOR" in
        # If the minor version isn't a number we cannot compare it, so we assume the tag uses the
        # current file naming scheme.
        '' | *[!0-9]*) ;;
        *)
            if [ "$MINOR" -lt 2 ]; then
                OLD_FILE_NAMING="true"
            fi
            # With no patch number, as in `v0.0`, `${MINOR_AND_REST#*.}` would give us the minor
            # version instead.
            if [ "$MINOR" -eq 0 ] && [ "$MINOR_AND_REST" != "$MINOR" ]; then
                V0_0_PATCH="${MINOR_AND_REST#*.}"
                case "$V0_0_PATCH" in
                '' | *[!0-9]*) V0_0_PATCH="" ;;
                esac
            fi
            ;;
        esac
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
        # The old file name for this CPU had no libc, unlike the other gnu targets.
        if [ "$OS" = "Linux" ] && [ -z "$OLD_FILE_NAMING" ]; then
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

    # Without this, the script would ask for a file that doesn't exist and fail with a 404, which
    # doesn't tell the user why.
    case "$CPU" in
    mips*)
        # The MIPS builds were all for Linux.
        if [ "$OS" != "Linux" ]; then
            echo "bootstrap-ubi.sh: Cannot determine what binary to download for your CPU architecture: $ARCH"
            exit 4
        fi
        if [ -z "$V0_0_PATCH" ] || [ "$V0_0_PATCH" -lt 20 ] || [ "$V0_0_PATCH" -gt 28 ]; then
            echo "bootstrap-ubi.sh: Only ubi releases v0.0.20 to v0.0.28 have builds for your CPU architecture: $ARCH"
            echo "bootstrap-ubi.sh: You can set TAG to one of those releases to install it."
            exit 4
        fi
        ;;
    esac

    if [ "$OS" = "Linux" ] && [ -n "$V0_0_PATCH" ]; then
        if [ "$V0_0_PATCH" -le 10 ]; then
            LIBC=""
            if [ "$CPU" = "arm" ]; then
                CPU="armv7"
            fi
        elif [ "$CPU" = "i686" ] && [ "$V0_0_PATCH" -ge 20 ] && [ "$V0_0_PATCH" -le 23 ]; then
            CPU="i586"
        fi
    fi

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
# With `set -e`, a failing `curl` would end the script here with `curl`'s own exit code, which can
# be the same as ours.
CURL_EXIT=0
STATUS=$(curl --silent --show-error --location --output "$LOCAL_FILE" --write-out "%{http_code}" "$URL") ||
    CURL_EXIT=$?
if [ "$CURL_EXIT" -ne 0 ]; then
    >&2 echo "curl failed to download $URL - curl exit code = $CURL_EXIT"
    exit 5
# Real `curl` always prints a status code, but a wrapper or an odd build might not.
elif [ -z "$STATUS" ]; then
    >&2 echo "curl failed to download $URL and did not print a status code"
    exit 5
elif [ "$STATUS" != "200" ]; then
    >&2 echo "curl failed to download $URL - status code = $STATUS"
    exit 6
fi

if echo "$FILENAME" | grep "\\.tar\\.gz$"; then
    EXE="ubi"
    tar -xzf "$LOCAL_FILE" "$EXE"
else
    EXE="ubi.exe"
    # The zip also has docs, which don't belong in the install directory. Without `-o`, `unzip` asks
    # before it replaces an existing `ubi.exe`, and when this script is piped to `sh`, it reads the
    # rest of the script as its answers.
    unzip -o "$LOCAL_FILE" "$EXE"
fi

chmod +x "$EXE"

rm -rf -- "$TEMPDIR"

echo ""
echo "bootstrap-ubi.sh: ubi has been installed to \`$TARGET\`."

# We match with `case` instead of `grep` because BusyBox `grep` doesn't support all of GNU grep's
# options, and because `grep` would treat `$TARGET` as a regex. Quoting it here makes it match as
# plain text.
case ":$PATH:" in
*":$TARGET:"*) ;;
*)
    echo "bootstrap-ubi.sh: It looks like \`$TARGET\` is not in your PATH. You may want to add it to use ubi."
    ;;
esac

echo ""
