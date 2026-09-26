#!/bin/sh
# Engine smoke test without any client: with its input closed, nh-engine
# must still greet (hello), publish the catalog, report the lost client and
# exit cleanly (bye).
#   sh tests/smoke.sh <build dir> [file to keep the session in]
set -eu
build=$(cd "$1" && pwd)
keep=${2:-}
pg=$(mktemp -d)
trap 'rm -rf "$pg"' EXIT
fail() { echo "smoke: $1" >&2; cat "$pg/stderr" >&2 2>/dev/null || true; exit 1; }
cp "$build"/data/nhdat "$build"/data/license "$build"/data/symbols \
   "$build"/data/sysconf "$pg"/
mkdir "$pg/save"
for f in perm record logfile xlogfile livelog; do : > "$pg/$f"; done
out="$pg/session.jsonl"
(cd "$pg" && \
    NETHACKOPTIONS='time,!legacy,!tutorial,!autopickup,name:Hero,role:valkyrie,race:human,gender:female,align:neutral' \
    RENETHACK_SEED=42 RENETHACK_FIXED_TIME=1768694400 \
    "$build/nh-engine" < /dev/null > "$out" 2> "$pg/stderr") \
    || fail "nh-engine exited with status $?"
head -1 "$out" | grep -q '^{"t":"hello","a":{"protocol":1,' || fail "no hello first"
sed -n 2p "$out" | grep -q '^{"t":"catalog",' || fail "no catalog second"
grep -q '"msg":"client closed the connection"' "$out" || fail "no error on EOF"
tail -1 "$out" | grep -q '^{"t":"bye"' || fail "no bye last"
ls "$pg/save" | grep -q . || fail "game not saved on EOF"
# whatever the game prints by itself (terminal NetHack writes --version to
# stdout) must stay inside the protocol
(cd "$pg" && "$build/nh-engine" --version < /dev/null > "$pg/version.out" \
    2> /dev/null) || true
[ -s "$pg/version.out" ] || fail "no output for --version"
if grep -qv '^{"t":"' "$pg/version.out"; then fail "non-protocol text on stdout"; fi
[ -z "$keep" ] || cp "$out" "$keep"
echo "smoke: ok ($(wc -l < "$out" | tr -d ' ') lines)"
