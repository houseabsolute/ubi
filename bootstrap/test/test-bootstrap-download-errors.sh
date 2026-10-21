#!/bin/sh

# bootstrap-ubi.sh uses exit code 5 when `curl` fails and 6 when the server doesn't return the file.
# We can't make the real `curl` fail on demand, so we run the script with a fake `curl` that acts
# like it couldn't connect, or couldn't look up the host, or got a 404.

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
-s) echo "Linux" ;;
-m) echo "x86_64" ;;
*) exit 1 ;;
esac
EOF

# Real `curl` prints `000` for `%{http_code}` when it gets no response, and then exits with its own
# error code.
cat >"$WORK/fake-bin/curl" <<'EOF'
#!/bin/sh
printf "%s" "$FAKE_CURL_STATUS"
if [ "$FAKE_CURL_EXIT" -ne 0 ]; then
    echo "curl: ($FAKE_CURL_EXIT) $FAKE_CURL_ERROR" >&2
fi
exit "$FAKE_CURL_EXIT"
EOF

chmod +x "$WORK/fake-bin/uname" "$WORK/fake-bin/curl"

FAILED=0

main() {
    check_download 000 7 "Failed to connect to github.com port 443" 5 "curl exit code = 7"
    check_download 000 6 "Could not resolve host: github.com" 5 "curl exit code = 6"
    check_download "" 0 "" 5 "did not print a status code"
    check_download 404 0 "" 6 "status code = 404"

    if [ "$FAILED" -ne 0 ]; then
        exit 1
    fi

    echo "bootstrap-ubi.sh reported each download error correctly"
}

# Takes the status code the fake `curl` prints, its exit code and its error message, then the exit
# code and message we expect from the script.
check_download() {
    STATUS=0
    env -u TAG -u FILENAME PATH="$WORK/fake-bin:$PATH" TARGET="$WORK/target" \
        FAKE_CURL_STATUS="$1" FAKE_CURL_EXIT="$2" FAKE_CURL_ERROR="$3" \
        sh "$SCRIPT" >"$WORK/out" 2>&1 || STATUS=$?
    if [ "$STATUS" -ne "$4" ] || ! grep -q "$5" "$WORK/out"; then
        cat "$WORK/out"
        echo "When curl printed \"$1\" and exited with $2, bootstrap-ubi.sh exited with $STATUS," \
            "not $4 with \"$5\""
        FAILED=1
    fi
}

main
