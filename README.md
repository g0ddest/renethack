# renethack

NetHack 5.0 with a modern RPG presentation. The game rules are NetHack's own,
unchanged; this repository adds a protocol host around the engine and, later,
a Godot client. Design: `docs/superpowers/specs/2026-09-26-renethack-design.md`.

## Build and test (macOS, Linux)

Requirements: a C compiler, GNU make, git, curl, Rust (stable).

    git submodule update --init
    make          # engine/build/nh-engine and engine/build/data
    make test     # C unit tests, engine smoke test, Rust tests
    make lint

The first build downloads Lua 5.4.8 (NetHack's Makefile checks its sha256).

## Playing a script

    cd client/rust
    printf 'key #\next quit\nyn y\nyn q\n' > /tmp/quit.script
    cargo run -p nh-cli -- run /tmp/quit.script --seed 42 --fixed-time 1768694400 \
        --record /tmp/quit.rhrec
    cargo run -p nh-cli -- replay /tmp/quit.rhrec

Script steps are documented on `nh_link::parse_script`.

## Layout

- `engine/upstream` — NetHack at tag `NetHack-5.0.0_Released` (never edited)
- `engine/patches` — the only changes to NetHack, applied at build time
- `engine/host` — the protocol host (`nh-engine`)
- `client/rust` — `nh-protocol`, `nh-link`, `nh-cli`

## License

NetHack General Public License (see `engine/upstream/dat/license`).
cJSON: MIT (`engine/host/third_party/cjson/LICENSE`).
