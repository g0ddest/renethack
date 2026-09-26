#!/bin/sh
# Engine smoke test without any client: with its input closed, nh-engine
# must still greet (hello), publish the catalog, report the lost client and
# exit cleanly (bye) -- and the same seed and clock must give the same game
# whatever the time zone or the player's ~/.nethackrc.
#   sh tests/smoke.sh <build dir> [file to keep the session in]
set -eu
build=$(cd "$1" && pwd)
keep=${2:-}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
fail() { echo "smoke: $1" >&2; cat "$tmp"/*/stderr >&2 2>/dev/null || true; exit 1; }

# run_engine NAME [VAR=value ...]: a fresh playground $tmp/NAME, then one
# session there with stdin closed; the assignments go into its environment
run_engine() {
    pg=$tmp/$1
    shift
    mkdir -p "$pg/save"
    cp "$build"/data/nhdat "$build"/data/license "$build"/data/symbols \
       "$build"/data/sysconf "$pg"/
    for f in perm record logfile xlogfile livelog; do : > "$pg/$f"; done
    (cd "$pg" && env \
        NETHACKOPTIONS='time,!legacy,!tutorial,!autopickup,name:Hero,role:valkyrie,race:human,gender:female,align:neutral' \
        RENETHACK_SEED=42 RENETHACK_FIXED_TIME=1768694400 "$@" \
        "$build/nh-engine" < /dev/null > "$pg/session.jsonl" 2> "$pg/stderr") \
        || fail "nh-engine exited with status $? ($pg)"
}

run_engine base
out=$tmp/base/session.jsonl
head -1 "$out" | grep -q '^{"t":"hello","a":{"protocol":1,' || fail "no hello first"
sed -n 2p "$out" | grep -q '^{"t":"catalog",' || fail "no catalog second"
if grep -q 'Unknown option' "$out"; then fail "engine rejected an option"; fi
grep -q '"msg":"client closed the connection"' "$out" || fail "no error on EOF"
tail -1 "$out" | grep -q '^{"t":"bye"' || fail "no bye last"
ls "$tmp/base/save" | grep -q . || fail "game not saved on EOF"

# whatever the game prints by itself (terminal NetHack writes --version to
# stdout) must stay inside the protocol
(cd "$tmp/base" && "$build/nh-engine" --version < /dev/null \
    > "$tmp/version.out" 2> /dev/null) || true
[ -s "$tmp/version.out" ] || fail "no output for --version"
if grep -qv '^{"t":"' "$tmp/version.out"; then fail "non-protocol text on stdout"; fi

# a fixed clock means the same game in every time zone (moon phase, night)
run_engine tz TZ=Pacific/Honolulu
cmp -s "$out" "$tmp/tz/session.jsonl" || fail "the time zone changes the game"

# the player's own ~/.nethackrc must not reach the engine
mkdir -p "$tmp/home"
echo 'OPTIONS=pettype:none' > "$tmp/home/.nethackrc"
run_engine rc HOME="$tmp/home"
cmp -s "$out" "$tmp/rc/session.jsonl" || fail "~/.nethackrc changes the game"

[ -z "$keep" ] || cp "$out" "$keep"
echo "smoke: ok ($(wc -l < "$out" | tr -d ' ') lines)"
