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

## Settings

The settings page (Settings on the title screen, the gear among the HUD's
buttons) shows a section at a time, each setting a row with its value
between arrows: ↑↓ (the d-pad) choose a row, ←→ change it, Enter (A)
steps it on, Tab or PgUp/PgDn (the bumpers) turn to another section, Esc
(B) goes back; the mouse clicks the arrows and the tabs. A change applies
at once, the scene behind the page showing it, and is kept in the profile
(`profile.json` in the playground), the same for every character; the next
start applies it again (a self-test plays with the defaults).

Display: the window, a borderless full screen (the display's mode kept)
or an exclusive one; the window's size, among those that fit the screen
(until one is chosen the window keeps the size it opens with); vsync; a
frame-rate cap (none, 30, 60, 120 or 144 frames a second).

Graphics: a preset, Low, Medium, High or Ultra, sets ambient occlusion,
indirect light, volumetric fog, shadows (their atlas, how soft their
edges are, how many of the torches nearest the hero cast them), MSAA, the
3D view's resolution (50–100 % of the window's) and how it is brought up
to the window's (bilinear, AMD FSR 1.0 or FSR 2.2); a control changed by
hand makes it Custom. High is the look the game was made with and a
desktop's default; a Steam Deck (Steam's `SteamDeck=1`, or a 1280×800
screen) starts on Medium, whose torches but the nearest cast no shadow.

Sound: the volumes of the mixer's buses (`audio.rs`), each a slider the
mouse drags. Interface: the scale (Auto, or 80–140 %;
`RENETHACK_UI_SCALE` overrides it for a run), the keys a new character
starts with, the language. Controls: what the keys do, for either key
profile, and the gamepad's buttons in the controller's own names
(`bindings.rs`; read only, as the tables above say).

The `settings` scenario changes them with the keys, the mouse's slider
and the gamepad (the window's size and mode headless only: a window keeps
the size its screenshots are taken at), and checks what Godot, the map
and the mixer were told, what the profile keeps and what a start reads
back. The `presets` scenario times the graphics on a game's first level,
each preset in turn with vsync off (`RENETHACK_BENCH` takes other
variants: `medium msaa=off;high ssil=off`). It is a measurement: run it
in release, in a window the screen shows (a hidden window draws nothing,
and the scenario says so), at the size it is for:

    godot --path client/godot -- --selftest=presets --size=1280x800

## Credits and licences

Credits on the title screen (and a button on the settings page) opens the
credits and the licences: the game, NetHack and its DevTeam, the art, the
sounds, the help's Guidebook and the Russian translation, as their
`CREDITS.md` files say (embedded when the extension is built, so the
screen never drifts from them; a CC-BY model is credited there as its
licence asks); then the package's notice (`tools/dist/legal/NOTICE.txt`),
the NGPL as the engine ships it (`data/license`), Godot's licence and the
third-party code in it (as the running Godot tells them), the Rust crates
linked into the extension (`build.rs` lists them from `cargo metadata`, as
`tools/dist/rust_licenses.py` does for legal/), Lua and cJSON, and the
fonts' SIL Open Font License. The page's words are in the player's
language, the attributions and licences as they are written. ↑↓ (the
d-pad) choose, PgUp/PgDn (the bumpers) scroll, Esc (B) goes back; a link
opens in the browser. The `credits` scenario checks that each document
shows what it must.

## Build and test

    git submodule update --init
    make              # engine/build/nh-engine, recover and data
    make test         # C unit tests, engine smoke test, Rust tests
    make lint
    make test-client  # self-tests of the Godot client: headless, two in a window (needs Godot)
    make soak         # random play through the client, seeds 1..8 (long)
    make deck         # screenshots at the Steam Deck's 1280×800 (needs a display)

`make test-client` runs each scenario of `client/rust/renethack-gd/src/selftest.rs`
in its own headless Godot process with a fixed seed: every scenario except
`tour`, a walk through the first rooms for map screenshots, and `gallery`,
the art laid out page by page. The `moves` scenario stops each of the first
steps midway (with `--screenshots`, a picture of the hero and the pet
between cells), each ring under its creature in every frame. The `passing`
scenario draws on the map what the engine draws in passing (a thrown dagger
on the hero's cell, a ray over the pet, a sparkle over the hero, the cell
the hero steps off a frame before the next): the hero and the pet stay,
their rings under them and the hero on the inventory's doll. The `through`
scenario puts a giant mummy in the cell south of the hero, between them and
the camera: the hero's outline shows through it, as through a wall; a newt
there, or the giant north of them, hides nothing. The `orders`
scenario walks, holds, counts, stops a walk with a key, takes the stairs,
and meets a hostile that stops a walk (with `--screenshots`, the way
previewed and the combat banner). The `inventory`
scenario opens the panel, filters it, wields by a drag and by the context
menu, answers a getobj question in selection mode and drops through the
panel's `D` menu; `bar` binds the food ration to a slot, follows it through
`#adjust`, eats it with `2`, and counts 20 searches with `n20s` (Modern) and
`Alt`+`2` `Alt`+`0` `s` (Classic). The `title` scenario checks the scene
behind the title menu: drawn before the start-up veil lifts, the hero in
their gear, both ends of the camera's sway, put away by a game and laid
again on the title after it. The `riding` scenario puts a Knight on his
pony on the first turn (debug mode): seated on its back in his kit, the
pony walking and galloping under him, a blow and the lance couched from the
saddle, the pony a pet with its saddle once he is off, then a warhorse
ridden through a save and a restore. The `states` scenario (debug mode)
shows what holds the hero, which their own glyph hides from the map: an
archeologist digs a pit and stands in it to the waist, then climbs out; is
held crouching by the shut jaws of a bear trap, wrapped struggling in a
web, sunk in lava and under water; then kneels to pray, too soon, and
darkness closes in round them, and again with the god forced pleased, in a
shaft of light; last drinks potions wished for and, after each spell of
helplessness, is shown in it: asleep on the floor with letters rising,
passed out drunk, slumped and paralysed. With `--screenshots` the soak saves the screen
every 60 answers (`soak-<seed>-<answers>.png`) and the first dialogs of each
kind as the player meets them, each question once
(`dlg-<kind>-<seed>-<answers>.png`). A picture waits for the screen to hold
still, as a player sees it who looks before they act: the answer played
out, the camera on the hero, nobody between two cells, an order walked to
its end (two and a half seconds at most; without `--screenshots` the soak
waits for nothing). The soak fails when the engine calls impossible()
("Program in disorder!"). The scenarios take screenshots under a display:

    cd client/godot
    godot --path . -- --selftest=smoke --screenshots=/tmp/shots --playground=/tmp/pg

A picture is the frame as it is drawn: it is taken when the next frame
begins, once Godot has laid out the containers and redrawn the controls of
the frame (taken in the middle of a frame, it showed a panel opened in that
frame with what it drew the time before, its list not laid out). The
`first-open` scenario checks the first frame drawn of the inventory panel
and of the command palette, the first time in a session and again.
Screenshots are 1920×1080 unless `--size=1280x800` (or Godot's own
`--resolution`) asks for another window; a run fails when the window or a
screenshot is not the size asked for, and at a set size every screen a
scenario shoots must fit the canvas (the HUD's blocks inside it and apart,
the log in whole lines). `make test-client` also runs `smoke`, `inventory`,
`hud`, `gamepad`, `title`, `threat-arrow` and `settings` headless at
1280×800, the Steam Deck's screen with its 120 % UI scale; `make deck`
shoots `smoke`, `tour`, `inventory`, `bar`, `hud`, `dialogs`, `gamepad` and
`settings` at a real 1280×800.

A headless run draws nothing, so what upsets Godot's renderer never shows
in one. `make test-client` therefore ends with two scenarios in a window
(they need a display; `xvfb-run make test-client` without one, or
`make test-client WINDOW_SELFTESTS=` to skip them). In `rim` the hero's rim
light's layer stays on the hero's own model, and a model in view changes
layers without leaving the renderer a pair it cannot undo. In `doll` the
inventory's doll shows the hero in its own lights (put out, the doll is
dim and the hero on the map as lit as before), from the first frame an
opening draws (the first time in a session, and after the hero walked
on), and without the tint their model wears on the map while a wall hides
them. Every run, headless or
not, passes only with its PASS line, exit status 0 and no error of Godot's
that says "BUG": a crash on quit, after the PASS line, fails it.
`make soak SOAK_WINDOW=1` plays the soak in a window, the renderer at work.

## Production builds

What a player installs, from Steam or a download. Each target builds into
`dist/build/<platform>` (kept, so the next run is incremental) and leaves
its package in `dist/`:

| Target | Package | Needs |
|---|---|---|
| `make dist-macos` | `renethack-<version>-macos-arm64.dmg` | an Apple silicon Mac, Xcode's command line tools |
| `make dist-linux` | `renethack-<version>-linux-x86_64.tar.gz` | Linux, in the Steam Linux Runtime 3 (sniper) SDK |
| `make dist-windows` | `renethack-<version>-windows-x86_64.zip` and `…-setup.exe` | Docker |

`make dist` builds this machine's own package and the Windows one. The CI
workflow builds all three (below).

They need Python 3 and Godot 4.7 (`GODOT=/Applications/Godot.app`) with
its export templates installed (Editor → Manage Export Templates, or the
`Godot_v<version>_export_templates.tpz` of the release unpacked into
`~/Library/Application Support/Godot/export_templates/<version>.stable/` on
macOS, `~/.local/share/godot/export_templates/<version>.stable/` on Linux).

Each target builds the engine (`-O2`, NetHack's hints for the platform) and
the extension (cargo's `dist` profile: release with whole-program LTO, no
symbols), exports a copy of `client/godot` staged with the version (the
project itself is never written to: the editor may have it open), and
packages the game with `engine/` (nh-engine, recover, data) and `legal/`.
The game finds the engine beside its executable (inside the app on macOS:
`renethack.app/Contents/Resources/engine`); `RENETHACK_ENGINE_DIR` still
overrides it. Saves stay in Godot's user data directory, as in development.

- The version is `git describe --tags` (a tag `v1.0.0` gives `1.0.0`),
  else `0.1.0+<commit>`, with `.dirty` for uncommitted changes. Its
  `x.y.z` and the commit count become the macOS bundle's version and the
  Windows file version.
- `STEAM=1` builds with the `steam` feature and puts Valve's steam_api
  library beside the extension. The App ID comes from Steam when it starts
  the game.
- `legal/` holds `NOTICE.txt` (what each part is and its licence), the
  NetHack General Public License, `engine-source.tar.gz` (the complete
  source of the engine binaries: NetHack at the commit built, the patches,
  the host, the build files and Lua, as the licence's paragraph 3(a) asks of
  a commercial distribution), the art, help and translation credits, the
  fonts' OFL texts, Godot's licence and notices, and the licences of the
  Rust crates in the extension.
- Before packaging, a target runs the packaged game's smoke self-test
  where the package can run (`DIST_SMOKE=0` skips it): a new game must
  start with the engine the package carries.
- The first build takes a while (the extension's LTO); the next one only
  redoes what changed, and the export and the packaging take a few
  minutes.

**macOS.** The dmg holds `renethack.app` and a link to `/Applications`;
`dist/build/macos/app/renethack.app` is the Steam depot. arm64 only (Apple
silicon; macOS 11 or newer). Its textures are S3TC/BPTC, as in development
and on the other platforms (Apple silicon reads them): Godot's own arm64
export would encode them again as ASTC at nearly twice the size, so the
stage asks for an x86_64 export from a template that holds the arm64
binary. Signing is ad-hoc unless:

- `MACOS_SIGN_IDENTITY="Developer ID Application: <name> (<team>)"` signs
  every binary with that identity, the hardened runtime and a timestamp
  (with the entitlements Steam's overlay needs,
  `tools/dist/macos/entitlements.plist`);
- `MACOS_NOTARY_PROFILE=<profile>` then notarizes and staples the app and
  the dmg; the profile is made once with
  `xcrun notarytool store-credentials <profile> --apple-id <id> --team-id <team>`.

An ad-hoc signed app runs from Steam; downloaded, macOS asks the player to
allow it once in System Settings → Privacy & Security.

**Linux.** The tar.gz holds a folder laid out as the Steam depot:
`renethack.x86_64` (the launch option), `renethack.pck`,
`librenethack_gd.so`, `engine/`, `legal/`. It is built in the Steam Linux
Runtime 3 (sniper) SDK, `registry.gitlab.steamos.cloud/steamrt/sniper/sdk`,
so its programs need glibc 2.31 at most, what the runtime Steam starts the
game in provides; the build prints what each binary needs.
`DIST_STEPS=native` or `package` runs half of it.

**Windows.** The zip holds the Steam depot (`renethack.exe`,
`renethack.pck`, `renethack_gd.dll`, `engine/` with `nh-engine.exe`,
`legal/`); the NSIS installer puts the same folder in Program Files, with a
Start menu entry and an uninstaller. The engine and the extension are
cross-built with mingw-w64 in Docker (`tools/dist/windows/Dockerfile`; the
engine is NetHack's own Windows configuration with the protocol host,
`make -C engine windows`). The smoke self-test runs under wine on x86_64
Linux. Nothing is Authenticode-signed yet.

**CI.** `.github/workflows/dist.yml` builds the packages on GitHub's
runners: Linux in the sniper SDK container, Windows with the same Docker
image, macOS (optional) on a macOS runner. Run it from the repository's
Actions tab (dist → Run workflow; tick Steam for a Steam build, macOS for
the dmg), or push a tag `v*` (a release: all three, with Steam). Each
job's package is an artifact of the run, on the run's page. Signing the
dmg takes the secrets `MACOS_CERTIFICATE` (a base64 .p12),
`MACOS_CERTIFICATE_PASSWORD` and `MACOS_SIGN_IDENTITY`, and notarizing it
`APPLE_ID`, `APPLE_TEAM_ID` and `APPLE_APP_PASSWORD`; without them it is
ad-hoc signed. `tools/dist/fetch_godot.sh` fetches Godot and its
templates (checked against the release's sums) where Godot is missing.

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

The game's own icon follows NetHack's: a shield with a blue field before
two crossed swords, drawn afresh in the game's steel and gold, with the
hero's `@` as the shield's device. `make app-icon` (needs Pillow) draws it
again:
`client/godot/icon.png` for the window, and `icon.icns` and `icon.ico` for
the Dock and the taskbar. Steamworks takes those two as its client icons
for macOS and Windows; `steam/icon/` has the rest it asks for, the client
icon for Linux (a zip of PNGs) and the 184×184 community icon.

The renderer is Forward+ (Vulkan, Metal or D3D12). `RENETHACK_FRAME_STATS=1`
logs frame times every two seconds; `RENETHACK_UI_SCALE` (percent, 80–140)
scales the interface over the settings' choice (Auto is 120 % on a
1280×800 screen).

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
  (what they may change: below)
- `engine/host` — the protocol host (`nh-engine`)
- `client/rust` — `nh-protocol`, `nh-link` (engine process, live sessions,
  saves), `nh-world` (world model, prompts, menus, key map), `nh-art` (the
  art manifest and its fallback chain), `nh-i18n` (the translation of the
  engine's texts), `nh-cli`, `renethack-gd` (the Godot extension)
- `client/i18n` — the catalog of the engine's texts and their translations
- `tools` — the art fetcher and the i18n extractor
- `client/godot` — the Godot project (a single scene; all logic is in Rust)
  and its art (`client/godot/art`)

The engine patches are thin: hooks through which the host observes the
game (what it says, what it shows) without changing it. One other kind is
allowed: a backport of an upstream fix that NetHack's own fixes file lists
for its next release (`doc/fixes5-0-1.txt`), when it changes no play: no
RNG, no game state, and the same soak digest with and without it (the
digest hashes every request of `--selftest=soak` and its answer). Such a
patch names its upstream commits in its header and is dropped when
`engine/upstream` moves to that release (`0004`: a vault guard parked off
the map made 5.0.0 call impossible()).

## License

NetHack General Public License (see `engine/upstream/dat/license`).
cJSON: MIT (`engine/host/third_party/cjson/LICENSE`).
Art: CC0; fonts: SIL OFL 1.1 (`client/godot/art/CREDITS.md`).
The Russian names: sources and credits in `client/i18n/CREDITS.md`.
The help's Guidebooks: `client/help/CREDITS.md` (NetHack General Public
License).
