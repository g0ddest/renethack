#!/bin/sh
# `make steampipe`: SteamPipe's build scripts for an upload to Steam, and a
# dry run over what they would upload. It uploads nothing: steamcmd runs
# the scripts, by hand (docs/steam.md, "Uploading").
#
# First it checks each depot's content: there, whole, the platform's own
# binaries, Steam's library beside them (a STEAM=1 build), the complete
# source in legal/, nothing that must not ship, one version in every depot. Then it fills
# steam/build/*.vdf.in into dist/steam/: app_build_<app>.vdf, and
# depot_build_<depot>.vdf for each platform given.
#
#   STEAM_APP_ID          the game's App ID (required)
#   STEAM_DEPOT_WINDOWS   each platform's depot ID; a platform without one
#   STEAM_DEPOT_MACOS     is left out of the build (one at least)
#   STEAM_DEPOT_LINUX
#   STEAM_CONTENT_<OS>    a depot's content other than the build's own: a
#                         folder, or a package (.zip, .tar.gz) to unpack.
#                         The build's own: dist/build/windows/pkg/renethack,
#                         dist/build/macos/app (renethack.app in it: the
#                         app, not the dmg), dist/build/linux/pkg/renethack;
#                         without it, the package in dist/ or a folder in
#                         it (Windows' or Linux', the CI workflow's), unpacked
#   STEAM_DESC            the build's description (renethack <version>)
#   STEAM_SET_LIVE        a beta branch to set the build live on (none; the
#                         default branch is set live in Steamworks, by hand)
#   STEAM_PREVIEW=0       steamcmd uploads; by default the scripts make a
#                         preview, its logs and manifest only
#   DIST_MAX_GLIBC        the newest glibc the Linux binaries may need
#                         (2.31: the Steam Linux Runtime 3, sniper)
. tools/dist/common.sh

S=$DIST/steam
problems=0
fail() { printf 'steampipe: %s\n' "$*" >&2; problems=$((problems + 1)); }
warn() { printf 'steampipe: warning: %s\n' "$*" >&2; }
# the value of the variable named $1, empty when unset
var() { eval "printf '%s' \"\${$1:-}\""; }
upper() { printf '%s' "$1" | tr a-z A-Z; }
steam_id() {
    case $2 in '' | *[!0-9]*) die "$1=$2: a Steam ID is a number" ;; esac
}

[ -n "${STEAM_APP_ID:-}" ] || die "STEAM_APP_ID: the game's App ID (Steamworks)"
steam_id STEAM_APP_ID "$STEAM_APP_ID"
PREVIEW=${STEAM_PREVIEW:-1}
case $PREVIEW in 0 | 1) ;; *) die "STEAM_PREVIEW=$PREVIEW: 0 or 1" ;; esac
SET_LIVE=${STEAM_SET_LIVE:-}
[ "$SET_LIVE" != default ] \
    || die "STEAM_SET_LIVE=default: Steamworks sets the default branch live, by hand"

OSES=
for os in windows macos linux; do
    d=$(var "STEAM_DEPOT_$(upper $os)")
    [ -n "$d" ] || continue
    steam_id "STEAM_DEPOT_$(upper $os)" "$d"
    [ "$d" != "$STEAM_APP_ID" ] || die "STEAM_DEPOT_$(upper $os): the App ID, not a depot's"
    for o in $OSES; do
        [ "$d" != "$(var "STEAM_DEPOT_$(upper "$o")")" ] \
            || die "STEAM_DEPOT_$(upper $os): the same depot as STEAM_DEPOT_$(upper "$o")"
    done
    OSES="$OSES $os"
done
[ -n "$OSES" ] || die "no depot: STEAM_DEPOT_WINDOWS, STEAM_DEPOT_MACOS or STEAM_DEPOT_LINUX"

# content <os>: CONTENT, the folder the depot is made of; a package (named,
# or the CI workflow's in dist/ or a folder in it) is unpacked into
# dist/steam/content/<os>
content() {
    os=$1
    CONTENT=
    case $os in
    windows) own=$DIST/build/windows/pkg/renethack pattern='renethack-*-windows-x86_64.zip' ;;
    macos) own=$DIST/build/macos/app pattern= ;;
    linux) own=$DIST/build/linux/pkg/renethack pattern='renethack-*-linux-x86_64.tar.gz' ;;
    esac
    src=$(var "STEAM_CONTENT_$(upper $os)")
    if [ -z "$src" ] && [ -d "$own" ]; then
        src=$own
    elif [ -z "$src" ] && [ -n "$pattern" ]; then
        # dist/ itself, or a folder in it (`gh run download -D dist`)
        for p in "$DIST"/$pattern "$DIST"/*/$pattern; do
            [ -f "$p" ] || continue
            [ -z "$src" ] || die "$os: more than one package in $DIST: name one, STEAM_CONTENT_$(upper $os)=..."
            src=$p
        done
    fi
    if [ -z "$src" ]; then
        case $os in
        macos) fail "macos: no app: STEAM=1 make dist-macos" ;;
        *) fail "$os: no package: the CI workflow's, from a STEAM=1 run, in $DIST" ;;
        esac
        return
    fi
    case $os:$src in
    macos:*.app | macos:*.app/) src=$(dirname "$src") ;;
    macos:*.dmg) die "macos: the depot is the app, not the dmg: STEAM_CONTENT_MACOS=<the folder holding renethack.app>" ;;
    esac
    if [ -f "$src" ]; then
        out=$S/content/$os
        rm -rf "$out"
        mkdir -p "$out"
        case $src in
        *.zip) unzip -q "$src" -d "$out" ;;
        *.tar.gz | *.tgz) tar -xzf "$src" -C "$out" ;;
        *) die "$os: $src: a folder, a .zip or a .tar.gz" ;;
        esac
        say "$os: $(basename "$src") unpacked"
        src=$out/renethack
    fi
    [ -d "$src" ] || { fail "$os: $src is not a folder"; return; }
    CONTENT=$(cd "$src" && pwd)
}

# Each takes the content's folder and paths in it.
need() {
    r=$1
    shift
    for f in "$@"; do
        [ -e "$r/$f" ] || fail "$os: $f is missing"
    done
}
runs() {
    r=$1
    shift
    for f in "$@"; do
        if [ ! -f "$r/$f" ]; then
            fail "$os: $f is missing"
        elif [ ! -x "$r/$f" ]; then
            fail "$os: $f is not executable"
        fi
    done
}
# kind <folder> <words> <path>...: `file` says each word of each
kind() {
    r=$1
    words=$2
    shift 2
    for f in "$@"; do
        [ -f "$r/$f" ] || continue
        k=$(file -b "$r/$f")
        for w in $words; do
            case $k in *"$w"*) ;; *) fail "$os: $f is not $words: $k" && break ;; esac
        done
    done
}
steam_lib() {
    [ -f "$1/$2" ] || fail "$os: no $(basename "$2"): a build without Steam (STEAM=1)"
}
# the complete source in the legal folder: the NGPL's 3(a) asks it of every
# copy sold in executable form (make dist-* packs it)
source_in() {
    set -- "$1/$2"/renethack-*-source.tar.gz
    [ -f "$1" ] || fail "$os: no renethack-<version>-source.tar.gz in legal/: the NGPL asks for it"
}

check_windows() {
    r=$1
    need "$r" renethack.pck engine/data/nhdat engine/data/sysconf legal/NOTICE.txt
    need "$r" renethack.exe renethack_gd.dll engine/nh-engine.exe engine/recover.exe
    kind "$r" "PE32+ x86-64" renethack.exe renethack_gd.dll engine/nh-engine.exe engine/recover.exe
    steam_lib "$r" steam_api64.dll
    source_in "$r" legal
}

check_linux() {
    r=$1
    need "$r" renethack.pck librenethack_gd.so engine/data/nhdat engine/data/sysconf legal/NOTICE.txt
    runs "$r" renethack.x86_64 engine/nh-engine engine/recover
    kind "$r" "ELF x86-64" renethack.x86_64 librenethack_gd.so engine/nh-engine engine/recover
    steam_lib "$r" libsteam_api.so
    source_in "$r" legal
    # what each binary needs of glibc: the runtime Steam starts it in has
    # no newer one
    max=${DIST_MAX_GLIBC:-2.31}
    if command -v objdump > /dev/null 2>&1; then
        for f in renethack.x86_64 librenethack_gd.so engine/nh-engine engine/recover; do
            [ -f "$r/$f" ] || continue
            g=$(objdump -T "$r/$f" 2> /dev/null | grep -o 'GLIBC_[0-9.]*' | sort -uV | tail -1)
            g=${g#GLIBC_}
            [ -z "$g" ] || [ "$(printf '%s\n' "$g" "$max" | sort -V | tail -1)" = "$max" ] \
                || fail "linux: $f needs glibc $g, the runtime has $max: build in the sniper SDK"
        done
    else
        warn "linux: no objdump here: the binaries' glibc unchecked"
    fi
}

check_macos() {
    r=$1
    a=renethack.app/Contents
    [ "$(ls -A "$r")" = renethack.app ] \
        || fail "macos: $r holds more than renethack.app: $(ls -A "$r" | tr '\n' ' ')"
    need "$r" $a/Info.plist $a/Resources/renethack.pck $a/Resources/engine/data/nhdat \
        $a/Resources/engine/data/sysconf $a/Resources/legal/NOTICE.txt $a/Frameworks/librenethack_gd.dylib
    runs "$r" $a/MacOS/renethack $a/Resources/engine/nh-engine $a/Resources/engine/recover
    kind "$r" "Mach-O arm64" $a/MacOS/renethack $a/Frameworks/librenethack_gd.dylib \
        $a/Resources/engine/nh-engine $a/Resources/engine/recover
    steam_lib "$r" $a/Frameworks/libsteam_api.dylib
    source_in "$r" $a/Resources/legal
    if command -v codesign > /dev/null 2>&1; then
        codesign --verify --deep --strict "$r/renethack.app" 2> /dev/null \
            || fail "macos: renethack.app's signature does not hold (codesign --verify --deep --strict)"
        case $(codesign -dv --verbose=2 "$r/renethack.app" 2>&1) in
        *'Authority=Developer ID Application'*)
            xcrun stapler validate -q "$r/renethack.app" > /dev/null 2>&1 \
                || warn "macos: signed with a Developer ID, not notarized (MACOS_NOTARY_PROFILE)" ;;
        *) warn "macos: signed ad hoc; a release wants a Developer ID and notarization" \
            "(MACOS_SIGN_IDENTITY, MACOS_NOTARY_PROFILE: make dist-macos)" ;;
        esac
    else
        warn "macos: no codesign here: the app's signature unchecked"
    fi
}

# What no depot ships. The depot's rules leave out a development App ID
# (steam_appid.txt) and Finder's .DS_Store, which in the app breaks its
# seal; a link to an absolute path would point nowhere on a player's
# machine.
litter() {
    r=$1
    f=$(find "$r" \( -name steam_appid.txt -o -name .DS_Store \) | head -3 | tr '\n' ' ')
    [ -z "$f" ] || warn "$os: left out of the depot: $f"
    find "$r" -type l | while read -r l; do
        case $(readlink "$l") in /*) printf '%s\n' "$l" ;; esac
    done > "$S/links"
    [ ! -s "$S/links" ] || fail "$os: links to absolute paths: $(head -3 "$S/links" | tr '\n' ' ')"
    rm -f "$S/links"
}

# version_of <folder>: the version the package's notice names
version_of() {
    case $os in
    macos) n=$1/renethack.app/Contents/Resources/legal/NOTICE.txt ;;
    *) n=$1/legal/NOTICE.txt ;;
    esac
    if [ -f "$n" ]; then sed -n '1s/^renethack //p' "$n"; fi
}

mkdir -p "$S"
rm -f "$S"/app_build_*.vdf "$S"/depot_build_*.vdf
VERSIONS=
DEPOTS=
for os in $OSES; do
    content "$os"
    [ -n "$CONTENT" ] || continue
    before=$problems
    "check_$os" "$CONTENT"
    litter "$CONTENT"
    v=$(version_of "$CONTENT")
    [ -n "$v" ] || { fail "$os: no version in its legal/NOTICE.txt"; v=unknown; }
    case $v in *dirty*) warn "$os: $v was built from a tree with changes not committed" ;; esac
    case " $VERSIONS " in *" $v "*) ;; *) VERSIONS="${VERSIONS:+$VERSIONS }$v" ;; esac
    files=$(find "$CONTENT" -type f | wc -l | tr -d ' ')
    size=$(du -sk "$CONTENT" | cut -f1)
    say "$os: depot $(var "STEAM_DEPOT_$(upper $os)"), $v, $files files, $((size / 1024)) MB:" \
        "$CONTENT$([ $problems = "$before" ] && echo || echo ' (FAILED)')"
    eval "CONTENT_$os=\$CONTENT"
done
case $VERSIONS in *' '*) fail "the depots are of different versions: $VERSIONS" ;; esac
[ "$problems" = 0 ] || die "$problems problem(s): no build scripts written, nothing to upload"

# fill <template> <out>: each ${NAME} the value of that variable, exported
fill() {
    awk '{
        line = $0; out = ""
        while (match(line, /[$][{][A-Z_]+[}]/)) {
            name = substr(line, RSTART + 2, RLENGTH - 3)
            if (!(name in ENVIRON)) { print FILENAME ": " name " unset" > "/dev/stderr"; exit 1 }
            out = out substr(line, 1, RSTART - 1) ENVIRON[name]
            line = substr(line, RSTART + RLENGTH)
        }
        print out line
    }' "$1" > "$2"
}

for os in $OSES; do
    DEPOT_ID=$(var "STEAM_DEPOT_$(upper $os)")
    DEPOT_CONTENT=$(var "CONTENT_$os")
    export DEPOT_ID DEPOT_CONTENT
    fill "$ROOT/steam/build/depot_build.vdf.in" "$S/depot_build_$DEPOT_ID.vdf"
    DEPOTS="${DEPOTS:+$DEPOTS
}		\"$DEPOT_ID\" \"depot_build_$DEPOT_ID.vdf\""
done
STEAM_DESC=${STEAM_DESC:-renethack $VERSIONS}
STEAM_PREVIEW=$PREVIEW
STEAM_SET_LIVE=$SET_LIVE
STEAM_CONTENT=$S/content
STEAM_OUTPUT=$S/output
STEAM_DEPOTS=$DEPOTS
export STEAM_APP_ID STEAM_DESC STEAM_PREVIEW STEAM_SET_LIVE STEAM_CONTENT STEAM_OUTPUT STEAM_DEPOTS
mkdir -p "$STEAM_CONTENT" "$STEAM_OUTPUT"
APP_BUILD=$S/app_build_$STEAM_APP_ID.vdf
fill "$ROOT/steam/build/app_build.vdf.in" "$APP_BUILD"

say "dry run passed: renethack $VERSIONS for$OSES; nothing uploaded"
say "$APP_BUILD: $([ "$PREVIEW" = 1 ] && echo 'a preview, steamcmd uploads nothing (STEAM_PREVIEW=0 for an upload)' || echo 'an upload')${SET_LIVE:+, set live on $SET_LIVE}"
printf '   steamcmd +login <account> +run_app_build %s +quit\n' "$APP_BUILD"
