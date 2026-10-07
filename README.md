# renethack

NetHack 5.0 with a modern RPG presentation. The game rules are NetHack's own,
unchanged; this repository adds a protocol host around the engine and a Godot
client written in Rust.

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

Every NetHack command works with its usual key. The key profile is chosen
when the character is created and kept with it:

| | **Modern** (default) | **Classic** |
|---|---|---|
| NetHack's `number_pad` | on | off |
| move | arrows, `Home` `PgUp` `End` `PgDn`, keypad; `Shift` — run | `hjklyubn` (and the arrows, keypad); `Shift` — run |
| a count (`20` searches) | `n20s` | `Alt`+`2` `Alt`+`0` `s` |
| `k` `j` `l` `u` | kick, jump, loot, untrap (number_pad letters) | moves |
| top-row `1`–`0` | the action bar | the action bar |

The client adds:

| Input | Action |
|---|---|
| `i` | the inventory panel (the engine's `i` is never needed): a grid in pack order, the paper doll, filters, the detail of an item. Double-click — its first action, right-click — all of them; drag to the doll — wear, wield, put on; off the doll — take off, remove, unwield; onto another item — `#adjust`; out of the panel — drop; onto the bar — bind. Command keys still work while it is open (`w`, then a letter). `Tab` filters, arrows move, `Enter` acts, `Space` the actions, `Esc` closes |
| a question about an item ("What do you want to wield?") | the panel in selection mode: the suggested items pulse, the others are dimmed but can be chosen; a click or the letter answers, `-` the hands, digits or `Shift`+click a count, `?`/`*` the Suggested/All filter, `Esc` cancels |
| a menu of your own items (`D`, `A`, identify...) | the panel with check marks: the letter or a click toggles, digits a count, `Enter` confirms |
| `1`–`9`, `0` | action bar slots: an item with its action, a spell or a command; a new character gets its role's loadout. After a count (`n20`, `Alt`+digits) a slot takes it by a click. Right-click clears a slot (Undo in the notice); an item's slot follows it through new letters and names, and shows it greyed while it is not in the pack. The bar is kept with the character (`<name>.rhui.json` in the game directory) |
| holding a direction key (or `s`, `.`) | step (search, wait) again every tick until the key is let go |
| a count, then a direction, `s` or `.` | that many steps, searches or waits, one per tick |
| left click on the map | walk there by the known map, one step per tick; on an object — pick it up, on stairs — take them, on a closed door — open it, on a monster — attack it; on yourself — the engine's menu of actions here |
| right click on the map | the engine's menu of actions for that cell (open, kick, talk, look at...) |
| `<` / `>` away from the stairs | walk to the known stairs and take them |
| `F5` | rest (search) until HP and Pw are full |
| any key, a click, `Esc` | stop the order before its next step |
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

### Gamepad and Steam Deck

The whole game plays with a controller (Xbox, PlayStation or the Steam
Deck; the buttons on screen are drawn with that controller's letters or
shapes). A strip over the action bar says what the buttons do on the
screen at hand.

| Input | In the world | In menus, questions and the inventory |
|---|---|---|
| left stick, d-pad | walk in 8 directions; held — walk on, as a held key | move the focus (the menu row, the item, the doll's socket, the answer) |
| right stick | the cursor over the map (what is there; getpos moves the engine's cursor) | move the focus |
| A | the cursor cell's action, as a left click (walk, pick up, open, attack) | choose; toggle in a menu of several |
| B | Esc | back, cancel |
| X | search | the item's actions (inventory) |
| Y | inventory | pick the item up, then put it down where the focus is (a drag: to the doll, onto another item) |
| LB + A B X Y | action bar slots 1–4 | the previous filter or page |
| RB + A B X Y | action bar slots 5–8 | the next filter or page |
| LB or RB + d-pad ← → | the bar's second page: LB + A, B are slots 9 and 0 | |
| LT (hold) | the radial menu: the cursor cell's actions (the engine's menu of what can be done there), pick up, fight, kick, rest, pray, travel, save; a stick picks, letting go of LT runs it | |
| RT | fire | |
| Start | the command palette | confirm (a menu of several, the keyboard) |
| Back / View | the message history | |
| L3, R3 | the whole level; the cursor back on the hero | |

A direction question takes the stick; a text question opens a keyboard on
screen (Latin and Cyrillic, Y switches; on a Steam Deck the client also
asks Steam for its own keyboard).

Time runs like in BG3. While no hostile is in view (**exploring**, the
badge top right), an order — a click, a held key, a count, `F5`, `<`/`>` —
is carried out one engine action per tick (300 ms; `tick_ms` in the
`[renethack]` section of `client/godot/project.godot`, 150–500, or
`RENETHACK_TICK_MS`). The hover shows the way a click would take. An order
stops before its next step on anything worth a look: a hostile in view,
lost HP, hunger, a new condition, a message (not what a pet does), a trap,
another level, a step that went nowhere, a question from the game, or any
key, click or panel. With a hostile in view (**combat**, a banner when it
starts), every action waits for you: a click takes one step (the first
mark of the way is larger), a held key still repeats. The fight ends after
three turns without a hostile in view. Without the engine patch that marks
peaceful monsters, every monster but a pet counts as hostile until the
game calls it peaceful (`;` on it says so) — shopkeepers, watchmen and
priests excepted.

## Build and test

    git submodule update --init
    make              # engine/build/nh-engine, recover and data
    make test         # C unit tests, engine smoke test, Rust tests
    make lint
    make test-client  # headless self-tests of the Godot client (needs Godot)
    make soak         # random play through the client, seeds 1..8 (long)
    make deck         # screenshots at the Steam Deck's 1280×800 (needs a display)

`make test-client` runs each scenario of `client/rust/renethack-gd/src/selftest.rs`
in its own headless Godot process with a fixed seed: every scenario except
`tour`, a walk through the first rooms for map screenshots, and `gallery`,
the art laid out page by page. The `moves` scenario stops each of the first
steps midway (with `--screenshots`, a picture of the hero and the pet
between cells). The `orders` scenario walks, holds, counts, stops a walk
with a key, takes the stairs, and meets a hostile that stops a walk (with
`--screenshots`, the way previewed and the combat banner). The `inventory`
scenario opens the panel, filters it, wields by a drag and by the context
menu, answers a getobj question in selection mode and drops through the
panel's `D` menu; `bar` binds the food ration to a slot, follows it through
`#adjust`, eats it with `2`, and counts 20 searches with `n20s` (Modern) and
`Alt`+`2` `Alt`+`0` `s` (Classic). The `title` scenario checks the scene
behind the title menu: drawn before the start-up veil lifts, the hero in
their gear, both ends of the camera's sway, put away by a game and laid
again on the title after it. With `--screenshots` the soak saves the screen
every 60 answers (`soak-<seed>-<answers>.png`) and the first dialogs of each
kind as the player meets them, each question once
(`dlg-<kind>-<seed>-<answers>.png`). A picture waits for the screen to hold
still, as a player sees it who looks before they act: the answer played
out, the camera on the hero, nobody between two cells, an order walked to
its end (two and a half seconds at most; without `--screenshots` the soak
waits for nothing). The scenarios take screenshots under a display:

    cd client/godot
    godot --path . -- --selftest=smoke --screenshots=/tmp/shots --playground=/tmp/pg

Screenshots are 1920×1080 unless `--size=1280x800` (or Godot's own
`--resolution`) asks for another window; a run fails when the window or a
screenshot is not the size asked for, and at a set size every screen a
scenario shoots must fit the canvas (the HUD's blocks inside it and apart,
the log in whole lines). `make test-client` also runs `smoke`, `inventory`,
`hud`, `gamepad` and `title` headless at 1280×800, the Steam Deck's screen with its
120 % UI scale; `make deck` shoots `smoke`, `tour`, `inventory`, `bar`,
`hud`, `dialogs` and `gamepad` at a real 1280×800.

## Languages

The client's own words (buttons, hints, panels, tooltips) are Project
Fluent catalogs built into the extension: `client/i18n/en.ftl`, the
source, and `client/i18n/ru.ftl`. Code takes a string with
`tr!("hud-gold", gold = 12)`; a static label bound with `i18n::text` or
`i18n::tip` follows a switch of the language by itself. A new key goes into
both files: the tests check that both have the same keys with the same
arguments, that every key the code names is there and that none is left
unused. What the engine says (messages, names, menus, text windows) stays
English in the engine and in the client's state, and reaches the screen
through `i18n::engine`, where the engine's translator (`nh-i18n`) plugs in.

The language is picked in the settings (the gear among the HUD's buttons,
Settings on the title screen) and when a character is made; it switches at
once and is kept in the profile (`profile.json` in the playground) and in
the character's UI state. At start: `--lang=ru`, else `RENETHACK_LANG`,
else the profile, else the system's language. The `language` scenario
switches in the settings and back, the log keeping the engine's English.
Where the engine asks for the name of a thing in English (a wish, a
genocide, a polymorph, what to write with a magic marker), a language
other than English opens a picker instead: the lexicon's names
(`client/i18n/lexicon.ru.toml`) searched by any of their forms, and for a
wish a builder of count, blessing and enchantment that also reads them from
what is typed ("благословенный +2 длинный меч"); the engine is sent the
English ("blessed +2 long sword"), and "Ввести по-английски" gives the
plain question back. An engraving goes in Latin letters (the engine wears
an engraving away a byte at a time): Cyrillic is transliterated, and
"Элберет" is written "Elbereth". The `pickers` scenario plays them in a
debug-mode game.
The help (F1, or the `?` among the HUD's buttons) is NetHack's Guidebook
in the interface's language, by chapters, with a search: the English of
NetHack's own `doc/Guidebook.mn`, the Russian of Vadim Velikodniy's
translation (`client/help/CREDITS.md`). `make help` builds both from their
sources with `tools/help/guidebook.py`; `make help-check` runs its tests
and fails when a built book is stale. The `help` scenario searches both.
The engine's pictures and tables (the tombstone, `#vanquished`,
`#genocided`, `#overview`), whose lines the catalog knows only as layout,
are drawn by the client from their English (`layouts.rs`): the tombstone
as a carved stone on the end screen (`ui/ui_tombstone.gdshader`), the
lists as rows with each creature's map symbol, the overview as the
dungeons' levels with their features and where the hero is; the client's
words come from Fluent, the engine's names through the lexicon. The
`layouts` scenario plays them in a debug-mode game, to its death.
`--lang=qps` is a pseudo-language for the self-tests: words through the
catalogs show ⟦so⟧ and the engine's ⟪so⟫, and a screen a scenario shoots
fails on any word outside the marks. `make test-client` also runs
scenarios in Russian at 1280×800 and 1920×1080 (the layout must fit) and in
the pseudo-language; `make deck DECK_ARGS=--lang=ru` shoots the Deck's
screens in Russian.

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

- an object in the hero's hands or on their body: the `held` section, again
  by appearance only (the head noun of the appearance, else the class); it
  says the model, its grip on the bone, the sub-meshes to hide and the metal;
- what a worn thing does to the outfit: the `worn` section, by appearance
  too: the material of metal body armour over the tunic (chain mail, plate),
  the leather of gloves over the hands, the model of a thing hung round the
  neck (a stethoscope), fitted to each body.

The hero shows their gear from the inventory the host sends before each
input wait: the weapon in the right hand, the shield on the left forearm, a
lit lamp in the left hand (with its own light), the alternate weapon and
the quiver on the back, armour pieces on the outfit, mail and plate in their
metal, gloves on the hands, a stethoscope round the neck. Using an item
(quaff, read, zap, cast, eat, apply, throw, fire, wear, pick up, kick) plays
a clip with the item in hand and an effect.

The inventory icons are baked from the same art: `make icons` (needs a
display, about a minute) renders every object appearance tile into
`client/godot/art/icons/items/<tile>.png`; they are committed.

The renderer is Forward+ (Vulkan, Metal or D3D12). `RENETHACK_FRAME_STATS=1`
logs frame times every two seconds; `RENETHACK_UI_SCALE` (percent, 80–140)
scales the interface (120 % by default on a 1280×800 screen).

`cargo test -p nh-art -- --nocapture coverage_report` prints how many
monsters and object tiles resolve at each level. The `gallery` self-test lays
the art out for screenshots:

    godot --path client/godot -- --selftest=gallery --screenshots=/tmp/shots

The `bestiary` self-test shoots the creatures a game meets most, one by one
and close up (`bestiary-<name>.png`, for contact sheets). It then shoots, side
by side under the same light, the ones a player tells apart by their colour
(`colours.png`: the fungi, the ants, the newt, the worms). Last comes a
wizard's start with the kitten, at the distance a game begins at and as close
as a player zooms (`ingame-wizard*.png`).
`RENETHACK_DUMP_SIGHTINGS=<file>` makes the map append, once a turn, the
creatures on it and the model each is drawn as, a procedural body or a
scene (JSON lines): a soak over several seeds counts which bodies are seen
most.

## Translation

The engine stays English; the client translates what it shows. The pieces
(Russian in progress):

- `client/i18n/catalog.en.json` — every text the engine can show, as its
  printf format with the call sites and what each placeholder names
  (monster, object, word, number...): the pline family with the prefix
  vpline sees ("You hit %s."), Sprintf pieces, menus, questions, text
  windows and the texts of `dat/` (rumours, oracles, epitaphs, engravings,
  quest texts). `make i18n-catalog` extracts it again with
  `tools/i18n/extract.py` (Python 3, no dependencies); `make i18n-check`
  runs the extractor's tests and fails when the committed catalog is out
  of date.
- `client/i18n/ru/*.toml` — the Russian templates, keyed by the catalog's
  ids: `"Вы бьёте {1:acc}."`, `"{1} {1:gender|убит|убита|убито|убиты}!"`,
  `"{1} {1:plural|монета|монеты|монет}"` (the syntax is in
  `client/rust/nh-i18n/src/template.rs`).
- `nh-i18n` — the translator: a message with its format takes its template,
  any other text is matched against the whole catalog; the names in it are
  declined by the lexicon. Its tests lint every translation and keep
  `tests/snapshots/grammar.txt`, each template rendered with words of every
  gender and number (`UPDATE_SNAPSHOTS=1 cargo test -p nh-i18n --test
  snapshots` writes it again).

The coverage on the soak: `RENETHACK_DUMP_MESSAGES=<file>` makes the
`soak` self-test append every shown text to a file as JSON lines;

    cd client/rust
    cargo run --release -p nh-i18n --bin i18n-coverage -- <file> [--todo todo.toml 100]

reports the share of texts a template matches and the share translated,
the texts no template matches and the untranslated templates by how often
they were shown (`--todo` writes the most shown as stubs to translate).

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
  art manifest and its fallback chain), `nh-i18n` (the translation of the
  engine's texts), `nh-cli`, `renethack-gd` (the Godot extension)
- `client/i18n` — the catalog of the engine's texts and their translations
- `tools` — the art fetcher and the i18n extractor
- `client/godot` — the Godot project (a single scene; all logic is in Rust)
  and its art (`client/godot/art`)

## License

NetHack General Public License (see `engine/upstream/dat/license`).
cJSON: MIT (`engine/host/third_party/cjson/LICENSE`).
Art: CC0; fonts: SIL OFL 1.1 (`client/godot/art/CREDITS.md`).
The Russian names: sources and credits in `client/i18n/CREDITS.md`.
The help's Guidebooks: `client/help/CREDITS.md` (NetHack General Public
License).
