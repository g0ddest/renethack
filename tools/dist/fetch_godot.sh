#!/bin/sh
# Godot's editor for this machine and the export templates named, from the
# release on GitHub, checked against its SHA512 sums (the CI workflow; any
# machine without Godot):
#   tools/dist/fetch_godot.sh <version> <dir> <template file>...
# The editor goes in <dir> (GODOT=<dir>/Godot_v<version>-stable_linux.x86_64
# or <dir>/Godot.app), the templates where Godot looks for them. Nothing is
# fetched twice.
set -eu
v=$1
dir=$2
shift 2
rel=https://github.com/godotengine/godot/releases/download/$v-stable
case $(uname -s) in
Linux)
    editor=Godot_v$v-stable_linux.x86_64.zip
    templates=${XDG_DATA_HOME:-$HOME/.local/share}/godot/export_templates/$v.stable
    ;;
Darwin)
    editor=Godot_v$v-stable_macos.universal.zip
    templates="$HOME/Library/Application Support/Godot/export_templates/$v.stable"
    ;;
*)
    echo "fetch_godot: no Godot editor for $(uname -s)" >&2
    exit 1
    ;;
esac
mkdir -p "$dir" "$templates"

check() {
    sum=$(grep " $1\$" "$dir/SHA512-SUMS.txt" | cut -d' ' -f1)
    got=$( (sha512sum "$dir/$1" 2>/dev/null || shasum -a 512 "$dir/$1") | cut -d' ' -f1)
    [ -n "$sum" ] && [ "$sum" = "$got" ] || { echo "fetch_godot: $1: bad checksum" >&2; exit 1; }
}
# unpack <zip> <dir> [member...]
unpack() {
    zip=$1
    to=$2
    shift 2
    if command -v unzip >/dev/null 2>&1; then
        unzip -q -o "$zip" "$@" -d "$to"
    else
        python3 -c 'import sys, zipfile
z = zipfile.ZipFile(sys.argv[1])
for name in sys.argv[3:] or z.namelist():
    z.extract(name, sys.argv[2])' "$zip" "$to" "$@"
    fi
}

curl -sSfL -o "$dir/SHA512-SUMS.txt" "$rel/SHA512-SUMS.txt"
if [ ! -f "$dir/.editor-$v" ]; then
    curl -sSfL -o "$dir/$editor" "$rel/$editor"
    check "$editor"
    unpack "$dir/$editor" "$dir"
    rm "$dir/$editor"
    chmod +x "$dir"/Godot_v*_linux.x86_64 "$dir"/Godot.app/Contents/MacOS/Godot 2>/dev/null || true
    touch "$dir/.editor-$v"
fi
missing=
for t in "$@"; do
    [ -f "$templates/$t" ] || missing=1
done
if [ -n "$missing" ]; then
    tpz=Godot_v$v-stable_export_templates.tpz
    curl -sSfL -o "$dir/$tpz" "$rel/$tpz"
    check "$tpz"
    names=$(for t in "$@" version.txt; do printf 'templates/%s ' "$t"; done)
    # shellcheck disable=SC2086 # one argument per template
    unpack "$dir/$tpz" "$dir/tpz" $names
    for t in "$@" version.txt; do
        mv "$dir/tpz/templates/$t" "$templates/"
    done
    rm -rf "$dir/$tpz" "$dir/tpz"
    chmod +x "$templates"/linux_* "$templates"/windows_* 2>/dev/null || true
fi
