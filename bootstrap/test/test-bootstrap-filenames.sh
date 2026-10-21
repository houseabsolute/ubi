#!/bin/sh

# The release file names changed a few times before v0.2.1, and CI can only run the script on a few
# platforms. So we run it with fake `uname` and `curl` commands and check the file name it asks for
# against the real asset names of each release.

set -e

if [ -n "$UBI_DEBUG_BOOTSTRAP" ]; then
    set -x
fi

SCRIPT="$(cd "$(dirname "$0")/.." && pwd)/bootstrap-ubi.sh"

WORK=$(mktemp -d)
trap 'rm -rf -- "$WORK"' EXIT

mkdir "$WORK/fake-bin" "$WORK/target"

cat >"$WORK/fake-bin/uname" <<'EOF'
#!/bin/sh
case "$1" in
-s) echo "$FAKE_UNAME_S" ;;
-m) echo "$FAKE_UNAME_M" ;;
*) exit 1 ;;
esac
EOF

# This saves the URL and says the file was not found, so the script stops before it tries to unpack
# anything.
cat >"$WORK/fake-bin/curl" <<EOF
#!/bin/sh
for ARG in "\$@"; do
    URL="\$ARG"
done
echo "\$URL" >"$WORK/url"
printf 404
EOF

chmod +x "$WORK/fake-bin/uname" "$WORK/fake-bin/curl"

FAILED=0

main() {
    # The first numbered release used one name for each platform, with no libc.
    check_filename v0.0.1 Linux x86_64 ubi-Linux-x86_64.tar.gz
    check_filename v0.0.5 Darwin x86_64 ubi-Darwin-x86_64.tar.gz
    check_filename v0.0.10 Linux x86_64 ubi-Linux-x86_64.tar.gz
    check_filename v0.0.10 Linux aarch64 ubi-Linux-aarch64.tar.gz
    check_filename v0.0.10 Linux armv7l ubi-Linux-armv7.tar.gz
    # v0.0.11 added the libc.
    check_filename v0.0.11 Linux x86_64 ubi-Linux-x86_64-musl.tar.gz
    check_filename v0.0.11 Linux armv7l ubi-Linux-arm-musl.tar.gz
    check_filename v0.0.15 Linux aarch64 ubi-Linux-aarch64-musl.tar.gz
    # 32-bit x86 was built for i586 from v0.0.20 to v0.0.23.
    check_filename v0.0.20 Linux i686 ubi-Linux-i586-musl.tar.gz
    check_filename v0.0.23 Linux i686 ubi-Linux-i586-musl.tar.gz
    check_filename v0.0.24 Linux i686 ubi-Linux-i686-musl.tar.gz
    check_filename v0.0.20 Linux mips ubi-Linux-mips.tar.gz
    # The old powerpc64le name has no libc, but the other gnu targets do.
    check_filename v0.0.20 Linux ppc64le ubi-Linux-powerpc64le.tar.gz
    check_filename v0.1.1 Linux ppc64le ubi-Linux-powerpc64le.tar.gz
    check_filename v0.1.1 Linux ppc64 ubi-Linux-powerpc64-gnu.tar.gz
    check_filename v0.1.1 Linux s390x ubi-Linux-s390x-gnu.tar.gz
    check_filename v0.1.1 Linux i686 ubi-Linux-i686-musl.tar.gz
    check_filename v0.1.1 Darwin arm64 ubi-Darwin-aarch64.tar.gz
    check_filename v0.1.1 FreeBSD amd64 ubi-FreeBSD-x86_64.tar.gz
    check_filename v0.1.1 NetBSD amd64 ubi-NetBSD-x86_64.tar.gz
    check_filename v0.1.1 MINGW64_NT-10.0-19045 x86_64 ubi-Windows-x86_64.zip
    # A tag with no patch number should still use the old naming, with no v0.0.x exceptions.
    check_filename v0.0 Linux x86_64 ubi-Linux-x86_64-musl.tar.gz
    # v0.2.1 started the current naming.
    check_filename v0.2.1 Linux ppc64le ubi-Linux-gnu-powerpc64le.tar.gz
    check_filename v0.2.1 Linux x86_64 ubi-Linux-musl-x86_64.tar.gz
    check_filename v0.2.1 MINGW64_NT-10.0-19045 x86_64 ubi-Windows-msvc-x86_64.zip
    check_filename "" Linux aarch64 ubi-Linux-musl-arm64.tar.gz
    check_filename "" Darwin arm64 ubi-macOS-arm64.tar.gz

    if [ "$FAILED" -ne 0 ]; then
        exit 1
    fi

    echo "bootstrap-ubi.sh asked for the right file name for every release and platform"
}

# Takes the tag, the fake `uname -s` and `uname -m` output, and the file name the script should
# ask for.
check_filename() {
    rm -f "$WORK/url"
    # Exit code 6 means the download failed, which is what our fake `curl` makes happen.
    STATUS=0
    env -u FILENAME PATH="$WORK/fake-bin:$PATH" TARGET="$WORK/target" TAG="$1" \
        FAKE_UNAME_S="$2" FAKE_UNAME_M="$3" sh "$SCRIPT" >"$WORK/out" 2>&1 || STATUS=$?
    if [ "$STATUS" -ne 6 ] || [ ! -f "$WORK/url" ]; then
        cat "$WORK/out"
        echo "bootstrap-ubi.sh with TAG=$1 on $2 $3 exited with $STATUS before it downloaded anything"
        FAILED=1
        return
    fi

    GOT=$(basename "$(cat "$WORK/url")")
    if [ "$GOT" != "$4" ]; then
        echo "bootstrap-ubi.sh with TAG=$1 on $2 $3 asked for $GOT, not $4"
        FAILED=1
    fi
}

main
