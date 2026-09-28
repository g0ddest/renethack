# renethack

NetHack 5.0 with a modern RPG presentation. The game rules are NetHack's own,
unchanged; this repository adds a protocol host around the engine and a Godot
client written in Rust. Design: `docs/superpowers/specs/2026-09-26-renethack-design.md`.

The client draws a torchlit 3D dungeon ("dark realism": CC0 photo textures,
animated models, procedural bodies for everything without a model), with the
status HUD, the message log and every NetHack menu and question as a dialog.
The game can be played from character creation to the end.

## Requirements (macOS, Linux)

- a C compiler, GNU make, git, curl, `patch`, `shasum` (perl) or `sha256sum`;
- Rust 1.94 or newer (stable; gdext 0.5.5 needs it);
- for `make test-client` and `make soak`: GNU `timeout` (coreutils; on macOS
  `brew install coreutils` provides it as `gtimeout`, which is found too);
- [Godot](https://godotengine.org/download) 4.5 or newer (tested with 4.7.1),
  the standard build (not .NET). If `godot` is not on your `PATH`, pass it:
  `make run GODOT=/path/to/godot` (on macOS the app bundle works too:
  `make run GODOT=/Applications/Godot.app`).

## Play

    git submodule update --init
    make run

`make run` builds the engine (`engine/build`), the client extension
(`client/rust/target/debug/librenethack_gd.*`), imports the Godot project and
any new or changed art, and starts the game. The first engine build downloads Lua 5.4.8 (NetHack's
Makefile checks its sha256). NetHack's Makefiles print a few harmless lines
such as `nroff: not found` and `expr: syntax error`.

Saved games live in Godot's user data directory, in `playground/`:
`~/.local/share/godot/app_userdata/renethack/playground` on Linux,
`~/Library/Application Support/Godot/app_userdata/renethack/playground` on
macOS. Closing the window saves the game in progress; the title screen offers
to continue it. One window at a time uses a playground: a second one says so
and starts nothing. If the engine crashes, the client rebuilds the game with
NetHack's `recover` (progress since the last level change is lost).

## Controls

Every NetHack command works with its usual key (`number_pad` off: `hjklyubn`
move, `<`/`>` stairs, `i` inventory, `#` extended commands...). The client adds:

| Input | Action |
|---|---|
| arrows, `Home` `PgUp` `End` `PgDn`, keypad | move (diagonals on the four keys and the keypad); with `Shift` — run |
| left click on the map | travel there (adjacent: move or attack; on yourself: action menu) |
| right click on the map | look at the cell |
| mouse over the map | what is there (by appearance only) |
| mouse wheel, `Ctrl`+`-` / `Ctrl`+`=` | zoom out / in |
| `F8` or `Ctrl`+`0` | the whole known level; again (or a zoom) — back to the hero |
| `#` | command palette: type to filter, `Tab` completes, `Up`/`Down` choose |
| `Alt`+letter | meta commands (`M-p` pray, `M-e` enhance, ...) |
| `Ctrl`+letter | `^X` attributes, `^T` teleport, `^P` previous messages (opens the full log)... |
| `F9` | full message log |
| in menus | the item's letter picks or toggles; digits type a count; `.` all, `-` none, `@` invert; `Enter` confirms; `Esc` cancels |
| in questions | the answer's letter; `Enter`/`Space` — the default; `Esc` — cancel |

Commands work in any keyboard layout (letters are taken by key position).

## Build and test

    git submodule update --init
    make              # engine/build/nh-engine, recover and data
    make test         # C unit tests, engine smoke test, Rust tests
    make lint
    make test-client  # headless self-tests of the Godot client (needs Godot)
    make soak         # random play through the client, seeds 1..8 (long)

`make test-client` runs each scenario of `client/rust/renethack-gd/src/selftest.rs`
in its own headless Godot process with a fixed seed: every scenario except
`tour`, a walk through the first rooms for map screenshots, and `gallery`,
the art laid out page by page. With `--screenshots` the soak saves the screen
every 60 answers. The scenarios take
screenshots under a display:

    cd client/godot
    godot --path . -- --selftest=smoke --screenshots=/tmp/shots --playground=/tmp/pg

## Art

The art is CC0 and committed under `client/godot/art/cc0` (Poly Haven
textures and models, Quaternius characters, animations, monsters, animals
and props; authors in `client/godot/art/CREDITS.md`). `make art` fetches it
again (`tools/fetch_art.py`, needs Python 3 with Pillow) and checks every
download against `client/godot/art/art.lock.json`; `make client` then
imports what changed (Godot's headless import, incremental).

`client/godot/art/manifest.json` says what draws each thing, from the most
specific rule to the most general (crate `nh-art`):

- a monster: its name, else its class letter, else its body (serpent, flyer,
  blob, ghost, humanoid...), else a generic beast; the height follows the
  catalog size, a tint tells species of one model apart;
- an object: its appearance, else its class symbol, else a generic pouch;
  only the appearance tile is ever used, so a look never tells more than the
  appearance does;
- a map feature: its terrain (materials for floors, walls, doors...).

`cargo test -p nh-art -- --nocapture coverage_report` prints how many
monsters and object tiles resolve at each level. The `gallery` self-test lays
the art out for screenshots:

    godot --path client/godot -- --selftest=gallery --screenshots=/tmp/shots

## Playing a script without the client

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
- `client/rust` — `nh-protocol`, `nh-link` (engine process, live sessions,
  saves), `nh-world` (world model, prompts, menus, key map), `nh-art` (the
  art manifest and its fallback chain), `nh-cli`, `renethack-gd` (the Godot
  extension)
- `client/godot` — the Godot project (a single scene; all logic is in Rust)
  and its art (`client/godot/art`)

## License

NetHack General Public License (see `engine/upstream/dat/license`).
cJSON: MIT (`engine/host/third_party/cjson/LICENSE`).
Art: CC0 (`client/godot/art/CREDITS.md`).
