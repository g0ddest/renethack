# Steam

What the Steamworks setup needs from the game, and where the game stands.
The builds themselves are in the README's "Production builds".

## Steam Deck: the Verified checklist

Valve's criteria (partner.steamgames.com/doc/steamdeck/compat), each
checked against the game as it is. **Pass**: met, and how that is known.
**Unknown**: it needs a Deck (or Steam) to tell. **Fail**: not met.

| Criterion | Status | How it was checked |
|---|---|---|
| Physical controls: the default configuration reaches all content, with no setting to change | Pass | The whole game plays with a controller (README, "Gamepad and Steam Deck"): walking, the radial menu, menus, questions, the inventory and doll, the palette, the settings, the help, text. A pad becomes the active input by itself. The `gamepad` self-test plays them, also at 1280×800 and in Russian. |
| On-screen glyphs match the input in use; no keyboard or mouse glyphs while a pad is used | Pass | Glyphs in the controller's family (Xbox, PlayStation, Steam Deck, by the device's name; `gamepad.rs`). With a pad the action bar shows its chords instead of the digits, the dialogs and the inventory their pad hints, and a strip says what the buttons do. A pad Steam Input presents under another name gets the Xbox family: A, B, X and Y where the Deck has them. |
| Text input: a Steamworks keyboard, or one of the game's own that types the player's language with a controller | Pass | A text question with a pad opens the game's keyboard (Latin and Cyrillic: English and Russian) and, on a Deck or in Big Picture, asks Steam for its own (`steam://open/keyboard`, `dialogs.rs`). The engraving and wish pickers work from the pad too (`pickers` self-test). |
| Default resolution 1280×800 (or 1280×720) | Pass | Every screen is laid out for 1280×800 and checked there: `make test-client` runs 15 scenarios at 1280×800 and 18 in Russian, and fails a screen that does not fit; `make deck` shoots them. A Deck starts Borderless at its screen's own size until the player chooses a window (`video::is_deck`); elsewhere the game opens a 1600×900 window, which on a Deck gamescope would letterbox at 1280×720. The `deck` scenario, run as a Deck (`SteamDeck=1`), checks that the window is the screen's size with nothing letterboxed. |
| Text legible at 30 cm; no character under 9 px at 1280×800 (12 px recommended) | Pass | The canvas is 1920×1080 design pixels scaled to the window (0.667 at 1280×800) times the interface scale, 120 % on a Deck: 0.8 of the design size. The smallest text in the code is 13 design px, 10.4 px on the Deck (three labels and the menu marks); the key pills are 14 (11.2 px); the body is 17 (13.6 px). Under the 12 px Valve recommends: the 13 and 14 px labels. |
| A default configuration with a playable frame rate (30 fps at 800p) | Unknown | A Deck starts on the Medium preset (`video::DECK_QUALITY`; only the nearest torch casts shadows). The `presets` self-test times the presets at 1280×800 (`--selftest=presets --size=1280x800`, a release build), but only a Deck gives the numbers. **Routed**: run it on a Deck. |
| No warnings that the device is unsupported | Pass | The game checks no device and shows no such message. |
| No launcher (or a launcher controller-navigable) | Pass | None: Steam starts the game, which opens on its title. |
| Proton or the native Linux build | Pass by design | The Linux depot is a native build made in the Steam Linux Runtime 3 (sniper) SDK, glibc 2.31 at most (`make dist-linux` checks it). In Steamworks, the Linux launch option should run under "Steam Linux Runtime 3.0 (sniper)". Not yet run on a Deck. |
| Sleep and resume (not in Valve's list; a crash after it would block) | Unknown | By design, nothing measures wall-clock time across a suspend: the engine's hang watch (20 s) and every other wait use monotonic clocks (`Instant`), which stop while a Linux machine sleeps; the engine is a child process, suspended with the game. The sounds' device after a resume is untried. **Routed**: suspend and resume a Deck mid-game, and with a dialog open. |

Steamworks settings for the Deck:
- Steam Input: the Deck's default configuration "Gamepad with Mouse
  Trackpad" (the game reads a gamepad, and the right trackpad as the
  mouse helps on the map).
- Linux launch option: the native build, `renethack.x86_64`, with the
  compatibility tool "Steam Linux Runtime 3.0 (sniper)".

Not a criterion: a PlayStation pad that Steam Input presents as its
virtual Xbox-style pad gets Xbox letters; Steam Input's glyph API
(`SteamInput()->GetInputTypeForHandle`) would name the real controller.

## Steam Cloud

Godot keeps the game's files in its user data directory, `user://`:

| OS | Directory | Auto-Cloud root and subdirectory |
|---|---|---|
| Windows | `%APPDATA%\Godot\app_userdata\renethack\` | WinAppDataRoaming, `Godot/app_userdata/renethack` |
| macOS | `~/Library/Application Support/Godot/app_userdata/renethack/` | MacAppSupport, `Godot/app_userdata/renethack` |
| Linux, Steam Deck | `$XDG_DATA_HOME/godot/app_userdata/renethack/` (`~/.local/share/…`) | LinuxXdgDataHome, `godot/app_userdata/renethack` |

The custom user directory (`application/config/use_custom_user_dir`) is
still the owner's choice to make; with it the subdirectory becomes
`renethack` on every OS, and the rules below change with it. Decide it
before the first release: moving it later strands the players' saves.

What is there, and what syncs:

| Path (under the directory) | What | Sync |
|---|---|---|
| `playground/save/*` | the saves on macOS and Linux: `<uid><name>`, compressed (`.Z` on macOS, `.gz` on Linux); a panic save ends in `.e` | yes |
| `playground/*.NetHack-saved-game` | the saves on Windows (`<name>` %-encoded) | yes |
| `playground/*.rhui.json` | each character's own state: the key profile, the action bar | yes |
| `playground/renethack-names.json` | the names as typed, for the save list | yes |
| `achievements.json` | the achievements earned, with their dates (Steam keeps its own) | yes |
| `playground/player.json` | the player's settings: the language, the keys, the game's options, the volumes, the last hero | yes |
| `playground/device.json` | this machine's: its window, its graphics preset, the interface's scale | never |
| `playground/bon*` | bones: a dead hero's level, met once by a later game, then deleted | never |
| `playground/<slot>.<n>` (`alock.0`…) | a running or crashed game's level files; the next start recovers them | never |
| `playground/record`, `logfile`, `xlogfile`, `livelog` | the scores and the game logs, appended | no |
| `playground/nhdat`, `license`, `symbols`, `sysconf` | the engine's data, copied from the install at every start | never |
| `playground/perm`, `renethack.lock`, `engine.stderr` | locks and the engine's error output | never |
| `logs/`, `shader_cache/`, `vulkan/` | Godot's logs and shader caches (tens of MB) | never |

Why not the rest:
- **Bones** belong to the machine: a bones file is met once, then deleted.
  Synced, a deleted file would come back from the cloud and be met again.
  Each one is also only valid for the engine version that wrote it.
- **Scores**: Steam Cloud replaces whole files, so two machines that each
  appended a game would lose one in a conflict. They stay per machine.
- **`device.json`** is one machine's: synced, a desktop's Ultra and 80 %
  would follow a player to their Deck. The player's own settings are in
  `player.json`, which syncs. (Older versions kept both in one
  `profile.json`; the first start of a newer one splits it, once, and
  keeps a `player.json` that Steam Cloud brought first.)

The rules (Steamworks → Application → Steam Cloud → Auto-Cloud), with no
root overrides:

| Root | Subdirectory | Pattern | OS | Recursive |
|---|---|---|---|---|
| WinAppDataRoaming | `Godot/app_userdata/renethack/playground` | `*.NetHack-saved-game` | Windows | no |
| WinAppDataRoaming | `Godot/app_userdata/renethack/playground` | `*.rhui.json` | Windows | no |
| WinAppDataRoaming | `Godot/app_userdata/renethack/playground` | `renethack-names.json` | Windows | no |
| WinAppDataRoaming | `Godot/app_userdata/renethack/playground` | `player.json` | Windows | no |
| WinAppDataRoaming | `Godot/app_userdata/renethack` | `achievements.json` | Windows | no |
| MacAppSupport | `Godot/app_userdata/renethack/playground/save` | `*` | macOS | no |
| MacAppSupport | `Godot/app_userdata/renethack/playground` | `*.rhui.json` | macOS | no |
| MacAppSupport | `Godot/app_userdata/renethack/playground` | `renethack-names.json` | macOS | no |
| MacAppSupport | `Godot/app_userdata/renethack/playground` | `player.json` | macOS | no |
| MacAppSupport | `Godot/app_userdata/renethack` | `achievements.json` | macOS | no |
| LinuxXdgDataHome | `godot/app_userdata/renethack/playground/save` | `*` | Linux | no |
| LinuxXdgDataHome | `godot/app_userdata/renethack/playground` | `*.rhui.json` | Linux | no |
| LinuxXdgDataHome | `godot/app_userdata/renethack/playground` | `renethack-names.json` | Linux | no |
| LinuxXdgDataHome | `godot/app_userdata/renethack/playground` | `player.json` | Linux | no |
| LinuxXdgDataHome | `godot/app_userdata/renethack` | `achievements.json` | Linux | no |

Quota: 200 files and 100 MB a user (a save is 50–300 KB compressed, up to
a few MB on Windows, where saves are not compressed).

Saves travel between machines of one OS: a Deck and a Linux desktop, two
Windows PCs. They do not travel between OSes. The rules above keep each
OS's files apart (no root overrides), on purpose:
- NetHack's save is the engine's structures as they lie in memory, and
  `long` is 64 bits on Linux and macOS but 32 on Windows.
- The names differ: `save/<uid><name>.gz` on Linux, `.Z` on macOS, and
  `<name>.NetHack-saved-game` on Windows.

A unix save also carries the user id in its name: the game lists the
saves of its own uid. The Deck's user (`deck`) is uid 1000, as most
desktop Linux users are; a desktop user with another uid would not see a
Deck's saves.

## Store art (drafts)

`steam/store/` holds draft art for the store page and the library, at the
sizes Steamworks asks for. They are drafts: the owner may replace any of
them with art of their own, under the same name and size.

`make steam-art` renders them in the game (it needs a display; half a
minute): the title's torchlit chamber on the Ultra preset, each from its
own camera, with a hero in the kit NetHack starts their role with, and the
game's name in the title's face (Cormorant SC, its gold) where Valve
allows text.

| File | Size | Steamworks | What |
|---|---|---|---|
| `header_capsule.png` | 920×430 | Store: header capsule | a Valkyrie, the name on the left |
| `small_capsule.png` | 462×174 | Store: small capsule | the name over the dimmed chamber |
| `main_capsule.png` | 1232×706 | Store: main capsule | a Knight, the name on the left |
| `vertical_capsule.png` | 748×896 | Store: vertical capsule | a Wizard, the name above |
| `page_background.png` | 1438×810 | Store: page background | the chamber from above, dimmed, no text |
| `library_capsule.png` | 600×900 | Library: capsule | a Samurai, the name below |
| `library_header.png` | 920×430 | Library: header | the store's header capsule |
| `library_hero.png` | 3840×1240 | Library: hero | the whole chamber, the hero in the middle, no text |
| `library_logo.png` | 1280×157, clear | Library: logo | the name alone; Steam lays it over the hero |

They keep to Valve's rules for these images: no text but the game's name
(no slogans, awards or review quotes), none at all on the library's hero
and the page's background, the name filling most of the small capsule,
and on the library's hero the hero's face and the altar inside the middle
860×380 pixels, which Steam never crops. Steamworks asks for PNGs for the
library's images; these are all PNGs.

Each run takes the same shots: the cameras, the heroes, the sizes and the
name's place are fixed in `store_art.rs`. What moves in the scene still
moves, the flames, the torches' flicker and the hero's breath, so a run
changes about one pixel in a hundred of the last one's: commit a new run
only when the art itself changed. `make steam-art` writes over every file
here: once a file holds the owner's art, take its shot out of `SHOTS`.
