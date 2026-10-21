#!/bin/sh

# The zip branch of bootstrap-ubi.sh only runs under Git Bash on Windows, and CI can't run that
# (see test-bootstrap.yml). So we run it on Linux with fake `uname` and `curl` commands that make
# the script think it's on Windows and hand it a local zip file shaped like a release. This needs
# `python3`, `timeout` and `unzip`.
#
# The release zip has docs next to `ubi.exe`, and only `ubi.exe` belongs in the install
# directory. We run the script twice, piped to `sh` the way the README says to, because on the
# second run `unzip` must not stop to ask whether to replace `ubi.exe`. In Git Bash, it would read
# the rest of the script from stdin as its answers. Other shells may read the whole script before
# running it, so `unzip` gets no answers and fails, which the test also catches.

set -e

if [ -n "$UBI_DEBUG_BOOTSTRAP" ]; then
    set -x
fi

SCRIPT="$(cd "$(dirname "$0")/.." && pwd)/bootstrap-ubi.sh"

WORK=$(mktemp -d)
trap 'rm -rf -- "$WORK"' EXIT

mkdir "$WORK/fake-bin" "$WORK/release" "$WORK/target"

echo "fake ubi" >"$WORK/release/ubi.exe"
echo "fake readme" >"$WORK/release/README.md"
echo "fake changes" >"$WORK/release/Changes.md"
# The dev container has no `zip` command, but it has Python.
python3 - "$WORK/release.zip" "$WORK/release" <<'EOF'
import os
import sys
import zipfile

with zipfile.ZipFile(sys.argv[1], "w") as z:
    for name in ("ubi.exe", "README.md", "Changes.md"):
        z.write(os.path.join(sys.argv[2], name), name)
EOF

cat >"$WORK/fake-bin/uname" <<'EOF'
#!/bin/sh
case "$1" in
-s) echo "MINGW64_NT-10.0-19045" ;;
-m) echo "x86_64" ;;
*) exit 1 ;;
esac
EOF

cat >"$WORK/fake-bin/curl" <<EOF
#!/bin/sh
while [ \$# -gt 0 ]; do
    if [ "\$1" = "--output" ]; then
        cp "$WORK/release.zip" "\$2"
    fi
    shift
done
printf 200
EOF

chmod +x "$WORK/fake-bin/uname" "$WORK/fake-bin/curl"

run_bootstrap() {
    # `timeout` stops the test from hanging if `unzip` waits for an answer.
    # `TAG` and `FILENAME` change which file the script asks for, so we make sure they're unset.
    if ! env -u TAG -u FILENAME PATH="$WORK/fake-bin:$PATH" TARGET="$WORK/target" \
        timeout 60 sh <"$SCRIPT" >"$WORK/out" 2>&1; then
        cat "$WORK/out"
        echo "Run $1 of bootstrap-ubi.sh failed"
        exit 1
    fi
    if grep -q "replace" "$WORK/out"; then
        cat "$WORK/out"
        echo "Run $1 of bootstrap-ubi.sh asked whether to replace a file"
        exit 1
    fi

    FILES=$(ls -A "$WORK/target")
    if [ "$FILES" != "ubi.exe" ]; then
        echo "Run $1 of bootstrap-ubi.sh left these files in the install directory:"
        echo "$FILES"
        exit 1
    fi
}

run_bootstrap 1
run_bootstrap 2

echo "bootstrap-ubi.sh installed only ubi.exe from a zip file, twice"
