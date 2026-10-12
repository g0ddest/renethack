#!/bin/sh
# Inside the Windows build image (tools/dist/windows.sh runs it): the engine
# and the extension for Windows x86_64, cross-built with mingw-w64.
#   /src      the repository, read-only
#   /in       upstream.tar and upstream.sha: NetHack at the submodule's commit
#   /build    a volume: build trees and cargo's target, kept between runs
#   /out      what the host packages
# STEAM=1: the steam feature and Valve's steam_api64.dll.
set -eu

# the engine's Makefile exports NetHack from a git checkout
up=/build/upstream
if [ ! -f "$up/.git/upstream.sha" ] || ! cmp -s /in/upstream.sha "$up/.git/upstream.sha"; then
    rm -rf "$up"
    mkdir -p "$up"
    tar -x -C "$up" -f /in/upstream.tar
    git -C "$up" init -q
    git -C "$up" add -A
    git -C "$up" -c user.name=dist -c user.email=dist@localhost commit -q -m upstream
    cp /in/upstream.sha "$up/.git/upstream.sha"
fi

# the engine's own files, writable, their times kept so make stays incremental
mkdir -p /build/engine
for f in Makefile patches host runtime tests; do
    rm -rf "/build/engine/$f"
    cp -Rp "/src/engine/$f" /build/engine/
done
# this machine's build first (the data files, the generated sources), then
# NetHack's WIN32 configuration with mingw-w64
make -C /build/engine BUILD=/build/engine/build UPSTREAM="$up" CCFLAGS=-O2 windows

CARGO_TARGET_DIR=/build/target cargo build --locked --profile dist -p renethack-gd \
    --target x86_64-pc-windows-gnu ${STEAM:+--features steam} \
    --manifest-path /src/client/rust/Cargo.toml

rm -rf /out/engine /out/lib /out/lua
mkdir -p /out/lib
cp -R /build/engine/build/windows /out/engine
rm -rf /out/engine/obj /out/engine/include
cp /build/target/x86_64-pc-windows-gnu/dist/renethack_gd.dll /out/lib/
if [ -n "${STEAM:-}" ]; then
    cp "$(ls -t /build/target/x86_64-pc-windows-gnu/dist/build/steamworks-sys-*/out/steam_api64.dll | head -1)" /out/lib/
fi
cp -R "$(ls -d /build/engine/build/tree/lib/lua-*/ | head -1)" /out/lua
# what the binaries import: system DLLs only (no mingw runtime DLL to ship)
for f in /out/engine/nh-engine.exe /out/engine/recover.exe /out/lib/renethack_gd.dll; do
    printf '%s: ' "${f##*/}"
    x86_64-w64-mingw32-objdump -p "$f" | sed -n 's/^\tDLL Name: //p' | sort -u | tr '\n' ' '
    echo
done
