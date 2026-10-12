#!/bin/sh
# `make dist-linux`: dist/renethack-<version>-linux-x86_64.tar.gz, a folder
# laid out as the Steam depot (the launch option: renethack.x86_64).
#
# It runs on Linux, in the Steam Linux Runtime 3 (sniper) SDK, so that the
# engine and the extension need no newer glibc than the runtime Steam
# starts the game in (2.31): the CI workflow (.github/workflows/dist.yml)
# runs it in that image. Built elsewhere, it says which glibc it needs.
#
#   DIST_STEPS=native   only the engine and the extension (dist/build/linux/out)
#   DIST_STEPS=package  only the export and the package, from that out/
#   DIST_SMOKE=0        skip the packaged game's smoke self-test
#   DIST_MAX_GLIBC      the newest glibc the binaries may need (2.31)
. tools/dist/common.sh

version_info
B=$DIST/build/linux
OUT=$B/out
steps=${DIST_STEPS:-native package}
say "renethack $VERSION for Linux x86_64${STEAM:+, with Steam}"

case $steps in *native*)
    [ "$(uname -s)" = Linux ] || die "the Linux build runs on Linux (the CI workflow runs it)"
    say "building the engine ($(uname -m))"
    mkdir -p "$B"
    make -C engine BUILD="$B/engine" CCFLAGS=-O2 UPSTREAM="$UPSTREAM" > "$B/engine.log" 2>&1 \
        || { tail -40 "$B/engine.log" >&2; die "the engine build failed (log: $B/engine.log)"; }
    say "building the extension"
    target=${DIST_CARGO_TARGET_DIR:-$B/target}
    CARGO_TARGET_DIR=$target cargo build --quiet --locked --profile dist -p renethack-gd \
        ${STEAM:+--features steam} --manifest-path client/rust/Cargo.toml
    rm -rf "$OUT"
    mkdir -p "$OUT/engine/data" "$OUT/lib"
    cp "$B/engine/nh-engine" "$B/engine/recover" "$OUT/engine/"
    cp "$B"/engine/data/* "$OUT/engine/data/"
    strip "$OUT/engine/nh-engine" "$OUT/engine/recover"
    cp "$target/dist/librenethack_gd.so" "$OUT/lib/"
    if [ -n "$STEAM" ]; then
        cp "$(ls -t "$target"/dist/build/steamworks-sys-*/out/libsteam_api.so | head -1)" "$OUT/lib/"
    fi
    cp -R "$(ls -d "$B"/engine/tree/lib/lua-*/ | head -1)" "$OUT/lua"
    # the newest glibc each binary needs: what Steam's runtime has at most
    # (DIST_MAX_GLIBC, 2.31 for sniper)
    for f in "$OUT/engine/nh-engine" "$OUT/engine/recover" "$OUT/lib/librenethack_gd.so"; do
        need=$(objdump -T "$f" | grep -o 'GLIBC_[0-9.]*' | sort -uV | tail -1)
        printf '%s: %s %s\n' "${f##*/}" "$(uname -m)" "$need"
        [ "$(printf '%s\n' "${need#GLIBC_}" "${DIST_MAX_GLIBC:-2.31}" | sort -V | tail -1)" \
            = "${DIST_MAX_GLIBC:-2.31}" ] || die "${f##*/} needs $need: build in the sniper SDK"
    done
    ;;
esac

case $steps in *package*)
    need_godot
    [ -f "$OUT/lib/librenethack_gd.so" ] || die "no native build in $OUT (DIST_STEPS=native)"
    stage_project "$B/project" "$OUT/lib/librenethack_gd.so"
    pkg=$B/pkg/renethack
    rm -rf "$B/pkg"
    mkdir -p "$pkg"
    godot_export "$B/project" Linux "$pkg/renethack.x86_64"

    say "assembling the depot"
    cp -R "$OUT/engine" "$pkg/"
    legal_sources "$OUT/lua"
    legal_folder "$pkg/legal" x86_64-unknown-linux-gnu
    [ -z "$STEAM" ] || cp "$OUT/lib/libsteam_api.so" "$pkg/"
    tidy "$pkg"

    if [ "${DIST_SMOKE:-1}" != 0 ]; then
        if [ "$(uname -s)-$(uname -m)" = Linux-x86_64 ]; then
            say "the packaged game's smoke self-test"
            pg=$B/smoke
            rm -rf "$pg"
            mkdir -p "$pg"
            env -u RENETHACK_ENGINE_DIR timeout 600 "$pkg/renethack.x86_64" --headless -- \
                --selftest=smoke --playground="$pg/playground" > "$pg/smoke.log" 2>&1 || true
            grep -q 'SELFTEST PASS smoke' "$pg/smoke.log" \
                || { tail -40 "$pg/smoke.log" >&2; die "the packaged game failed its smoke self-test"; }
        else
            say "no smoke self-test here: the package runs on x86_64 Linux"
        fi
    fi

    TGZ=$DIST/renethack-$VERSION-linux-x86_64.tar.gz
    rm -f "$TGZ"
    pack_tgz "$B/pkg" renethack "$TGZ"
    say "$TGZ ($(du -h "$TGZ" | cut -f1))"
    ;;
esac
