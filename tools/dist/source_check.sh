#!/bin/sh
# `make source-check`: the source archive the last `make dist-*` packed
# (dist/build/source/) builds by itself, as it would for someone who has
# only that: unpacked away from the repository, the engine built from it
# with neither git nor the network and its tests run, the extension checked.
. tools/dist/common.sh

set -- "$DIST"/build/source/renethack-*-source.tar.gz
[ -f "$1" ] || die "no source archive in $DIST/build/source: make dist-* first"
[ $# = 1 ] || die "more than one source archive in $DIST/build/source"
archive=$1
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
say "unpacking $(basename "$archive")"
tar -xzf "$archive" -C "$work"
set -- "$work"/renethack-*
src=$1
[ -f "$src/SOURCE.txt" ] || die "the archive has no renethack-<version>/SOURCE.txt"

say "the engine"
make -C "$src/engine" > "$work/engine.log" 2>&1 \
    || { tail -40 "$work/engine.log" >&2; die "the engine does not build from the source"; }
if grep -q 'Trying https://' "$work/engine.log"; then
    die "the engine's build fetched Lua: the archive lacks it"
fi
make -C "$src/engine" test > "$work/test.log" 2>&1 \
    || { tail -40 "$work/test.log" >&2; die "the engine's tests fail on the source"; }

say "the extension"
(cd "$src/client/rust" \
    && CARGO_TARGET_DIR="$work/target" cargo check --locked --quiet -p renethack-gd) \
    || die "the extension does not compile from the source"
say "$(basename "$archive") builds"
