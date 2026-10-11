#!/bin/sh
# `make dist-macos`: dist/renethack-<version>-macos-arm64.dmg, the app and an
# /Applications link. dist/build/macos/app/renethack.app is the Steam depot.
#
#   MACOS_SIGN_IDENTITY   codesign identity (default "-", ad-hoc); a
#                         "Developer ID Application: ..." signs with the
#                         hardened runtime and a timestamp
#   MACOS_NOTARY_PROFILE  a notarytool keychain profile (xcrun notarytool
#                         store-credentials): the app and the dmg are
#                         notarized and stapled
#   DIST_SMOKE=0          skip the packaged game's smoke self-test
. tools/dist/common.sh

[ "$(uname -s)" = Darwin ] && [ "$(uname -m)" = arm64 ] \
    || die "dist-macos builds on an Apple silicon Mac"
need_godot
version_info
IDENTITY=${MACOS_SIGN_IDENTITY:--}
NOTARY=${MACOS_NOTARY_PROFILE:-}
[ -z "$NOTARY" ] || [ "$IDENTITY" != - ] \
    || die "notarization needs a Developer ID: MACOS_SIGN_IDENTITY=..."

B=$DIST/build/macos
TARGET=${DIST_CARGO_TARGET_DIR:-$B/target}
mkdir -p "$B"
# Godot's own minimum for arm64; clang and rustc take it from here
export MACOSX_DEPLOYMENT_TARGET=11.0
say "renethack $VERSION for macOS arm64${STEAM:+, with Steam}"

say "building the engine"
make -C engine BUILD="$B/engine" CCFLAGS=-O2 UPSTREAM="$UPSTREAM" > "$B/engine.log" 2>&1 \
    || { tail -40 "$B/engine.log" >&2; die "the engine build failed (log: $B/engine.log)"; }

say "building the extension"
CARGO_TARGET_DIR=$TARGET cargo build --quiet --profile dist -p renethack-gd \
    ${STEAM:+--features steam} --manifest-path client/rust/Cargo.toml
# its install name is the build's path otherwise
lib=$B/librenethack_gd.dylib
cp "$TARGET/dist/librenethack_gd.dylib" "$lib"
install_name_tool -id @rpath/librenethack_gd.dylib "$lib" 2>/dev/null
codesign --force --sign - "$lib" 2>/dev/null
if [ -n "$STEAM" ]; then
    steam_api=$(ls -t "$TARGET"/dist/build/steamworks-sys-*/out/libsteam_api.dylib | head -1)
    [ -n "$steam_api" ] || die "the steamworks-sys build left no libsteam_api.dylib"
fi

# Godot's arm64 export wants a thin template binary; its templates are
# universal: thinned here, once per Godot version
gv=$("$GODOT" --version | cut -d. -f1-4)
tzip="$HOME/Library/Application Support/Godot/export_templates/$gv/macos.zip"
[ -f "$tzip" ] || die "no export templates for Godot $gv (Editor > Manage Export Templates)"
thin=$B/template-$gv-arm64.zip
if [ ! -f "$thin" ]; then
    say "thinning Godot's macOS template to arm64"
    rm -rf "$B/template"
    mkdir -p "$B/template"
    unzip -q "$tzip" -d "$B/template"
    for v in release debug; do
        bin=$B/template/macos_template.app/Contents/MacOS/godot_macos_$v
        lipo -thin arm64 "$bin.universal" -output "$bin.arm64"
        rm "$bin.universal"
    done
    (cd "$B/template" && zip -q -r -y "$thin" macos_template.app)
    rm -rf "$B/template"
fi

STAGE_ASTC=1 STAGE_MACOS_TEMPLATE=$thin stage_project "$B/project" "$lib"
rm -rf "$B/app"
mkdir -p "$B/app"
APP=$B/app/renethack.app
godot_export "$B/project" macOS "$APP"

say "assembling the app"
res=$APP/Contents/Resources
mkdir -p "$res/engine/data"
cp "$B/engine/nh-engine" "$B/engine/recover" "$res/engine/"
cp "$B"/engine/data/* "$res/engine/data/"
strip "$res/engine/nh-engine" "$res/engine/recover"
legal_sources "$(ls -d "$B"/engine/tree/lib/lua-*/ | head -1)"
legal_folder "$res/legal" aarch64-apple-darwin
[ -z "$STEAM" ] || cp "$steam_api" "$APP/Contents/Frameworks/"
tidy "$APP"

say "signing ($IDENTITY)"
sign() {
    if [ "$IDENTITY" = - ]; then
        set -- --sign - "$@"
    else
        set -- --sign "$IDENTITY" --options runtime --timestamp "$@"
    fi
    out=$(codesign --force "$@" 2>&1) || { printf '%s\n' "$out" >&2; die "codesign failed"; }
}
# inside out: the libraries and the engine, then the app that seals them
for f in "$APP"/Contents/Frameworks/*.dylib "$res/engine/nh-engine" "$res/engine/recover"; do
    sign "$f"
done
sign --entitlements "$TOOLS/macos/entitlements.plist" "$APP"
codesign --verify --deep --strict "$APP"

notarize() {
    say "notarizing $(basename "$1")"
    xcrun notarytool submit "$1" --keychain-profile "$NOTARY" --wait
}
if [ -n "$NOTARY" ]; then
    ditto -c -k --keepParent "$APP" "$B/notarize.zip"
    notarize "$B/notarize.zip"
    rm -f "$B/notarize.zip"
    xcrun stapler staple "$APP"
fi

if [ "${DIST_SMOKE:-1}" != 0 ]; then
    say "the packaged game's smoke self-test"
    pg=$B/smoke
    rm -rf "$pg"
    mkdir -p "$pg"
    env -u RENETHACK_ENGINE_DIR /usr/bin/perl -e 'alarm shift; exec @ARGV' 300 \
        "$APP/Contents/MacOS/renethack" --headless -- --selftest=smoke \
        --playground="$pg/playground" > "$pg/smoke.log" 2>&1 || true
    grep -q 'SELFTEST PASS smoke' "$pg/smoke.log" \
        || { tail -40 "$pg/smoke.log" >&2; die "the packaged game failed its smoke self-test"; }
fi

say "making the dmg"
DMG=$DIST/renethack-$VERSION-macos-arm64.dmg
root=$B/dmg
rm -rf "$root" "$DMG"
mkdir -p "$root"
clone "$APP" "$root/"
ln -s /Applications "$root/Applications"
hdiutil create -quiet -volname renethack -srcfolder "$root" -fs HFS+ -format ULMO "$DMG"
rm -rf "$root"
if [ "$IDENTITY" != - ]; then
    codesign --sign "$IDENTITY" --timestamp "$DMG"
fi
if [ -n "$NOTARY" ]; then
    notarize "$DMG"
    xcrun stapler staple "$DMG"
fi
say "$DMG ($(du -h "$DMG" | cut -f1))"
