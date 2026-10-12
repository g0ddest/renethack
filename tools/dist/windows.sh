#!/bin/sh
# `make dist-windows`: dist/renethack-<version>-windows-x86_64.zip, a folder
# laid out as the Steam depot (the launch option: renethack.exe), and
# dist/renethack-<version>-windows-x86_64-setup.exe, an NSIS installer. The
# engine and the extension are cross-built with mingw-w64 in Docker
# (tools/dist/windows/Dockerfile, native on arm64 and x86_64 hosts); Godot on
# this machine exports the game.
#
#   DIST_SMOKE=0   skip the packaged game's smoke self-test, which runs
#                  under wine on an x86_64 Linux host that has it
. tools/dist/common.sh

command -v docker >/dev/null || die "dist-windows needs Docker"
need_godot
version_info
B=$DIST/build/windows
IMAGE=renethack-dist-windows
mkdir -p "$B/in" "$B/out"
say "renethack $VERSION for Windows x86_64${STEAM:+, with Steam}"

say "the build image"
docker build --quiet -t "$IMAGE" "$TOOLS/windows" > /dev/null

say "building the engine and the extension (Docker)"
git -C "$UPSTREAM" archive --format=tar HEAD > "$B/in/upstream.tar"
git -C "$UPSTREAM" rev-parse HEAD > "$B/in/upstream.sha"
docker run --rm -e STEAM="$STEAM" \
    -v "$ROOT:/src:ro" -v "$B/in:/in:ro" -v "$B/out:/out" \
    -v renethack-dist-windows-build:/build \
    -v renethack-dist-windows-cargo:/opt/cargo/registry \
    "$IMAGE" sh /src/tools/dist/windows/build.sh > "$B/native.log" 2>&1 \
    || { tail -40 "$B/native.log" >&2; die "the Windows build failed (log: $B/native.log)"; }
grep -E '^[a-z0-9_.-]+\.(exe|dll): ' "$B/native.log"

stage_project "$B/project" "$B/out/lib/renethack_gd.dll"
pkg=$B/pkg/renethack
rm -rf "$B/pkg"
mkdir -p "$pkg"
godot_export "$B/project" "Windows Desktop" "$pkg/renethack.exe"

say "assembling the depot"
cp -R "$B/out/engine" "$pkg/"
legal_sources "$B/out/lua"
legal_folder "$pkg/legal" x86_64-pc-windows-gnu
[ -z "$STEAM" ] || cp "$B/out/lib/steam_api64.dll" "$pkg/"
tidy "$pkg"

if [ "${DIST_SMOKE:-1}" != 0 ]; then
    if [ "$(uname -s)-$(uname -m)" = Linux-x86_64 ] && command -v wine >/dev/null; then
        # quiet, and no dialog offering to install Mono or Gecko
        export WINEDEBUG=-all WINEDLLOVERRIDES='mscoree=d;mshtml=d'
        wineboot --init > /dev/null 2>&1 || true
        # the engine alone first: engine/tests/smoke.sh through a wrapper
        say "the engine's smoke test (wine)"
        eng=$B/engine-smoke
        rm -rf "$eng"
        mkdir -p "$eng"
        cp -R "$pkg/engine/data" "$eng/"
        printf '#!/bin/sh\nexec wine "%s" "$@"\n' "$pkg/engine/nh-engine.exe" > "$eng/nh-engine"
        chmod +x "$eng/nh-engine"
        sh engine/tests/smoke.sh "$eng" || die "the Windows engine failed its smoke test"

        say "the packaged game's smoke self-test (wine)"
        pg=$B/smoke
        rm -rf "$pg"
        mkdir -p "$pg/playground"
        timeout 900 wine "$pkg/renethack.exe" --headless -- --selftest=smoke \
            --playground="$(winepath -w "$pg/playground")" > "$pg/smoke.log" 2>&1 || true
        # what Godot printed, else its own log in the player's AppData
        glog=$(find "${WINEPREFIX:-$HOME/.wine}/drive_c/users" -path '*app_userdata/renethack/logs/godot.log' 2>/dev/null | head -1)
        grep -q 'SELFTEST PASS smoke' "$pg/smoke.log" ${glog:+"$glog"} \
            || { tail -40 "$pg/smoke.log" >&2; die "the packaged game failed its smoke self-test"; }
    else
        say "no smoke self-test here: it runs under wine on x86_64 Linux"
    fi
fi

ZIP=$DIST/renethack-$VERSION-windows-x86_64.zip
SETUP=renethack-$VERSION-windows-x86_64-setup.exe
rm -f "$ZIP" "$DIST/$SETUP"
(cd "$B/pkg" && zip -q -r -X "$ZIP" renethack)
say "the installer (NSIS)"
docker run --rm -v "$B/pkg:/pkg:ro" -v "$ROOT:/src:ro" -v "$DIST:/dist" "$IMAGE" \
    makensis -V2 -DVERSION="$VERSION" -DVERSION4="$VERSION_NUM.$BUILD_NUM" \
    -DSRC=/pkg/renethack -DICON=/src/client/godot/icon.ico -DOUT="/dist/$SETUP" \
    /src/tools/dist/windows/installer.nsi > "$B/nsis.log" 2>&1 \
    || { cat "$B/nsis.log" >&2; die "makensis failed"; }
say "$ZIP ($(du -h "$ZIP" | cut -f1))"
say "$DIST/$SETUP ($(du -h "$DIST/$SETUP" | cut -f1))"
