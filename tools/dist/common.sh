# Shared by tools/dist/<platform>.sh, sourced from the repository root.
#
# Environment (all optional):
#   GODOT      the Godot 4.7 editor binary that exports (default: godot)
#   STEAM=1    build with the `steam` feature and ship Valve's steam_api
#   UPSTREAM   the NetHack checkout (default: engine/upstream)
#   DIST       where the packages go (default: dist); dist/build/<platform>
#              keeps the build trees, so a second run is incremental

set -eu

ROOT=$(pwd)
DIST=${DIST:-$ROOT/dist}
case $DIST in /*) ;; *) DIST=$ROOT/$DIST ;; esac
GODOT=${GODOT:-godot}
UPSTREAM=${UPSTREAM:-$ROOT/engine/upstream}
case $UPSTREAM in /*) ;; *) UPSTREAM=$ROOT/$UPSTREAM ;; esac
STEAM=${STEAM:-}
TOOLS=$ROOT/tools/dist

START=$(date +%s)
say() {
    t=$(($(date +%s) - START))
    printf '== [%d:%02d] %s\n' $((t / 60)) $((t % 60)) "$*"
}
die() { printf 'dist: %s\n' "$*" >&2; exit 1; }

# VERSION: `git describe --tags` without its "v", else 0.1.0+<sha>; ".dirty"
# (or git's "-dirty") when tracked files differ from HEAD.
# VERSION_NUM: its x.y.z (else 0.1.0). BUILD_NUM: the commit count.
version_info() {
    if VERSION=$(git describe --tags --match 'v[0-9]*' --dirty 2>/dev/null); then
        VERSION=${VERSION#v}
    else
        VERSION=0.1.0+$(git rev-parse --short=8 HEAD)
        git diff --quiet HEAD -- 2>/dev/null || VERSION=$VERSION.dirty
    fi
    VERSION_NUM=$(printf '%s\n' "$VERSION" | sed -nE 's/^([0-9]+\.[0-9]+\.[0-9]+).*/\1/p')
    [ -n "$VERSION_NUM" ] || VERSION_NUM=0.1.0
    BUILD_NUM=$(git rev-list --count HEAD)
}

need_godot() {
    command -v "$GODOT" >/dev/null 2>&1 || [ -x "$GODOT" ] \
        || die "Godot not found: GODOT=$GODOT (pass the 4.7 editor binary)"
    gv=$("$GODOT" --version 2>/dev/null | head -1)
    case $gv in
    4.7.*) ;;
    *) die "Godot $gv: the export templates are 4.7 (GODOT=...)" ;;
    esac
}

# stage_project <dir> <library>...: a copy of client/godot to export from,
# the extension's libraries in its bin/, version stamped. APFS clones on
# macOS (no disk), reflinks or plain copies elsewhere. The stage keeps its
# import cache between runs (seeded from the project's own), so Godot only
# imports what changed.
stage_project() {
    stage=$1
    shift
    src=$ROOT/client/godot
    mkdir -p "$stage"
    find "$stage" -mindepth 1 -maxdepth 1 ! -name .godot -exec rm -rf {} +
    if [ ! -d "$stage/.godot" ] && [ -d "$src/.godot" ]; then
        clone "$src/.godot" "$stage/"
        rm -rf "$stage/.godot/shots" "$stage/.godot/export_credentials.cfg"
    fi
    for f in "$src"/*; do
        case ${f##*/} in
        export_presets.cfg | project.godot | renethack.gdextension) continue ;;
        esac
        clone "$f" "$stage/"
    done
    mkdir -p "$stage/bin"
    for lib in "$@"; do
        cp "$lib" "$stage/bin/"
    done
    # the libraries from bin/, wherever the development build keeps them
    sed -E 's#res://\.\./rust/target/(debug|release)/#res://bin/#' \
        "$src/renethack.gdextension" > "$stage/renethack.gdextension"
    awk -v v="$VERSION" '
        /^config\/version=/ { next }
        { print }
        /^config\/name=/ { print "config/version=\"" v "\"" }
        ' "$src/project.godot" > "$stage/project.godot"
    # the versions; for the macOS preset STAGE_MACOS_TEMPLATE, the template,
    # and STAGE_MACOS_ARCH, the architecture asked for (macos.sh says why)
    awk -v num="$VERSION_NUM" -v build="$BUILD_NUM" -v tmpl="${STAGE_MACOS_TEMPLATE:-}" \
        -v arch="${STAGE_MACOS_ARCH:-}" '
        /^platform=/ { platform = $0 }
        /^\[preset\.[0-9]+\.options\]$/ { options = platform }
        /^application\/short_version=/ { $0 = "application/short_version=\"" num "\"" }
        /^application\/version=/ { $0 = "application/version=\"" build "\"" }
        /^application\/(file|product)_version=/ { sub(/=.*/, "=\"" num "." build "\"") }
        /^custom_template\/(release|debug)=/ && options == "platform=\"macOS\"" && tmpl != "" {
            sub(/=.*/, "=\"" tmpl "\"")
        }
        /^binary_format\/architecture=/ && options == "platform=\"macOS\"" && arch != "" {
            sub(/=.*/, "=\"" arch "\"")
        }
        { print }' "$src/export_presets.cfg" > "$stage/export_presets.cfg"
    say "importing the staged project"
    "$GODOT" --headless --path "$stage" --import > "$stage/../import.log" 2>&1 || true
}

# pack_tgz <dir> <name> <archive>: <dir>/<name> as a tar.gz, without macOS's
# extended attributes, owned by nobody in particular
pack_tgz() {
    if [ "$(uname -s)" = Darwin ]; then
        COPYFILE_DISABLE=1 tar --no-mac-metadata --no-xattrs --uid 0 --gid 0 \
            --uname root --gname root -C "$1" -czf "$3" "$2"
    else
        tar --owner=0 --group=0 --numeric-owner -C "$1" -czf "$3" "$2"
    fi
}

# tidy <dir>: what a package holds is readable by every user of the machine
# (an install another user made), its programs executable
tidy() {
    chmod -R u+rwX,go+rX,go-w "$1"
}

clone() {
    if [ "$(uname -s)" = Darwin ]; then
        cp -c -R -p "$1" "$2"
    else
        cp -a --reflink=auto "$1" "$2"
    fi
}

# godot_export <stage> <preset> <output>: a headless release export; a
# failed one exits non-zero. Its other errors are shown, but not the
# headless renderer's leak report at exit, nor this machine's Godot failing
# to load another platform's extension (the export only copies it).
godot_export() {
    say "exporting $2"
    log=$1/../export-$(printf '%s' "$2" | tr ' ' '-').log
    "$GODOT" --headless --path "$1" --export-release "$2" "$3" > "$log" 2>&1 \
        || { tail -40 "$log" >&2; die "the export of $2 failed (log: $log)"; }
    [ -e "$3" ] || die "the export of $2 made no $3 (log: $log)"
    grep -E '^(ERROR|SCRIPT ERROR):' "$log" | grep -vE 'leaked at exit|dynamic library|Error loading extension|Cannot get class .RenethackGame' >&2 || true
}

# legal_folder <dir> [the extension's target triple]: what must ship with
# the game (tools/dist/legal/NOTICE.txt says what each file is)
legal_folder() {
    out=$1
    rm -rf "$out"
    mkdir -p "$out/fonts"
    sed -e "s#@VERSION@#$VERSION#g" "$TOOLS/legal/NOTICE.txt" > "$out/NOTICE.txt"
    cp "$UPSTREAM/dat/license" "$out/NetHack-General-Public-License.txt"
    cp "$ROOT/engine/host/third_party/cjson/LICENSE" "$out/cJSON-LICENSE.txt"
    cp "$ROOT/client/godot/art/CREDITS.md" "$out/art-CREDITS.md"
    cp "$ROOT/client/godot/audio/CREDITS.md" "$out/audio-CREDITS.md"
    cp "$ROOT/client/help/CREDITS.md" "$out/help-CREDITS.md"
    cp "$ROOT/client/i18n/CREDITS.md" "$out/i18n-CREDITS.md"
    for d in "$ROOT"/client/godot/fonts/*/; do
        cp "$d/OFL.txt" "$out/fonts/$(basename "$d")-OFL.txt"
    done
    cp "$DIST/build/source/Lua-LICENSE.txt" "$DIST/build/source/engine-source.tar.gz" "$out/"
    cp "$DIST/build/source/Godot-LICENSE.txt" "$DIST/build/source/Godot-COPYRIGHT.txt" "$out/"
    python3 "$TOOLS/rust_licenses.py" --manifest-path "$ROOT/client/rust/Cargo.toml" \
        ${2:+--target "$2"} ${STEAM:+--features steam} > "$out/THIRD-PARTY-rust.txt"
}

# The source of the engine binaries (NGPL 3(a): the source accompanies
# them) and Lua's notice, once per version; Godot's notices, once per Godot;
# shared by the platforms. $1: the Lua source the engine build fetched.
legal_sources() {
    lua=$1
    src=$DIST/build/source
    mkdir -p "$src"
    stamp=$src/engine-source.version
    if [ ! -f "$stamp" ] || [ "$(cat "$stamp")" != "$VERSION" ]; then
        say "packing the engine's source"
        [ -f "$lua/src/lua.h" ] || die "no Lua source in $lua"
        name=renethack-engine-$VERSION
        rm -rf "${src:?}/$name" "$src/engine-source.tar.gz"
        mkdir -p "$src/$name/engine/upstream"
        git -C "$UPSTREAM" archive --format=tar HEAD | tar -x -C "$src/$name/engine/upstream"
        for f in Makefile patches host runtime tests; do
            cp -R "$ROOT/engine/$f" "$src/$name/engine/"
        done
        cp -R "$lua" "$src/$name/"
        cp "$TOOLS/legal/SOURCE.txt" "$src/$name/README.txt"
        pack_tgz "$src" "$name" "$src/engine-source.tar.gz"
        rm -rf "${src:?}/$name"
        sed -n '/^\* Copyright (C) 1994/,/^\*\*\*\*/p' "$lua/src/lua.h" \
            | sed -e 's/^\* \{0,1\}//' -e '/^\*\*\*/d' > "$src/Lua-LICENSE.txt"
        printf '%s\n' "$VERSION" > "$stamp"
    fi
    gv=$("$GODOT" --version 2>/dev/null | head -1)
    if [ ! -f "$src/Godot-COPYRIGHT.txt" ] || [ "$(cat "$src/godot.version" 2>/dev/null)" != "$gv" ]; then
        probe=$src/godot-licenses
        rm -rf "$probe"
        mkdir -p "$probe"
        printf 'config_version=5\n[application]\nconfig/name="licenses"\n' > "$probe/project.godot"
        "$GODOT" --headless --path "$probe" -s "$TOOLS/godot_licenses.gd" -- "$src" \
            > "$probe/log" 2>&1 || true
        [ -s "$src/Godot-COPYRIGHT.txt" ] || { cat "$probe/log" >&2; die "Godot's notices"; }
        rm -rf "$probe"
        printf '%s\n' "$gv" > "$src/godot.version"
    fi
}
