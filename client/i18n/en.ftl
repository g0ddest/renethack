# renethack: the client's own interface, in English (the source; ru.ftl
# has the same keys). The engine's words (messages, names, menus) are not
# here: nh-i18n translates those.

## The title, settings, character creation, the end of a game, errors

veil-first-launch = Preparing the game for its first launch…
title-subtitle = NetHack 5.0
title-new-game = New game
title-continue = Continue: { $name } ({ $time })
title-settings = Settings
title-credits = Credits
title-quit = Quit

settings-title = Settings
settings-back = Back
settings-credits = Credits and licences
settings-hint = ↑↓: choose · ←→: change · Tab: section · Esc: back
settings-display = Display
settings-interface = Interface
settings-on = On
settings-off = Off
settings-window-mode = Window
settings-window-windowed = Windowed
settings-window-borderless = Borderless full screen
settings-window-exclusive = Exclusive full screen
settings-window-mode-about = A window, or the whole screen: borderless keeps the desktop a keystroke away, exclusive gives the display to the game alone.
settings-window-size = Window size
settings-size = { $width } × { $height }
settings-window-size-about = The window's size, among those that fit the screen.
settings-window-size-fullscreen = A full screen has the screen's own size: the window's is chosen in a window.
settings-vsync = Vertical sync
settings-vsync-about = Frames shown in step with the screen, without tearing. Off, they come as fast as they are drawn, up to the cap.
settings-fps-cap = Frame-rate cap
settings-fps-none = None
settings-fps = { $fps } fps
settings-fps-cap-about = At most this many frames a second: a cooler, quieter machine and a longer battery.
settings-graphics = Graphics
settings-quality = Quality
settings-quality-low = Low
settings-quality-medium = Medium
settings-quality-high = High
settings-quality-ultra = Ultra
settings-quality-custom = Custom
settings-quality-about = A preset sets every control below; changing one by hand makes it Custom.
settings-ssao = Ambient occlusion
settings-ssao-about = Shade where the walls meet the floor and in the corners: the depth of the stone.
settings-ssil = Indirect light
settings-ssil-about = Torchlight thrown back off the walls and the floor: the warm light around the flames.
settings-fog = Volumetric fog
settings-fog-about = The haze that glows around the flames and hangs in the dark.
settings-shadows = Shadows
settings-shadows-about = The shadows' detail and softness, and how many of the torches nearest the hero cast them.
settings-msaa = Anti-aliasing (MSAA)
settings-samples = { $samples }×
settings-msaa-about = Smooth edges on the walls and the models; each step asks more of the graphics card.
settings-scale = Render scale
settings-percent = { $percent }%
settings-scale-about = The 3D view drawn at part of the window's resolution and brought up to it: faster, a little softer. The interface stays sharp.
settings-upscale = Upscaling
settings-upscale-bilinear = Bilinear
settings-upscale-fsr1 = AMD FSR 1.0
settings-upscale-fsr2 = AMD FSR 2.2
settings-upscale-about = How a 3D view drawn below the window's resolution is brought up to it. FSR 2.2 also smooths edges over frames, even at 100%.
settings-sound = Sound
settings-volume-master = Master volume
settings-volume-master-about = Every sound of the game at once.
settings-volume-music = Music
settings-volume-music-about = The music of the dungeon and its branches.
settings-volume-ambience = Ambience
settings-volume-ambience-about = The dungeon's own sounds: the wind, the water, the fire, the far echoes.
settings-volume-sfx = Effects
settings-volume-sfx-about = Blows, spells, doors, footsteps and the creatures' voices; what the hero only hears, too.
settings-volume-ui = Interface sounds
settings-volume-ui-about = The clicks of the buttons and the panels.
settings-ui-scale = Interface scale
settings-ui-scale-auto = Auto ({ $percent }%)
settings-ui-scale-about = The size of the words and the panels. Auto: 120% on a Steam Deck, else 100%.
settings-ui-scale-forced = Set by RENETHACK_UI_SCALE for this run.
settings-keys = Keys for new characters
settings-keys-modern = Modern
settings-keys-classic = Classic
settings-keys-about = { $keys }. A character keeps the keys it was made with.
settings-keys-playing-modern = This character plays with the Modern keys.
settings-keys-playing-classic = This character plays with the Classic keys.
settings-language = Language
settings-language-about = The language of the interface and of the game's texts.
settings-reduce-effects = Reduce screen effects
settings-reduce-effects-about = No sway of the hero, no drift of the camera, no shifting hues of a hallucination. Blindness still dims the view.
settings-hints = Hints
settings-hints-about = A card beside the HUD the first time something new comes up: a creature, a thing underfoot, hunger, stairs… The help's Guide keeps the ones you have met.
settings-order-speed = Order speed
settings-order-speed-about = How fast your own orders go — a walk to a click, a held key, F5's rest: a step at once (Instant), every 0.15 s (Fast), 0.3 s (Normal) or 0.5 s (Slow). A count is NetHack's and goes at once.
settings-order-speed-instant = Instant
settings-order-speed-fast = Fast
settings-order-speed-normal = Normal
settings-order-speed-slow = Slow
settings-stairs-walk = < and > walk to the stairs
settings-stairs-walk-about = Off the stairs, On walks to the known stairs and takes them; Off answers as NetHack does ("You can't go down here.").
settings-gameplay = Gameplay
settings-autopickup = Autopickup
settings-autopickup-about = Walking onto things picks up the kinds the next row chooses.
settings-pickup-types = Kinds picked up
settings-pickup-all = Everything
settings-pickup-coins = Coins
settings-pickup-valuables = Coins and small valuables
settings-pickup-custom = Classes { $symbols }
settings-pickup-types-about = What autopickup takes: everything, only coins, or coins with amulets, scrolls, spellbooks, potions, rings and wands.
settings-pickup-burden = Load lifted without asking
settings-burden-unencumbered = Unencumbered
settings-burden-burdened = Burdened
settings-burden-stressed = Stressed
settings-burden-strained = Strained
settings-burden-overtaxed = Overtaxed
settings-burden-overloaded = Overloaded
settings-pickup-burden-about = Picking up what would load the hero beyond this asks first.
settings-autoopen = Open doors by walking
settings-autoopen-about = Walking into a closed door opens it.
settings-autoquiver = Fill the quiver to fire
settings-autoquiver-about = Firing with nothing quivered first readies something fit to fire.
settings-safe-pet = Spare pets and peacefuls
settings-safe-pet-about = Walking into a pet or a peaceful creature in sight swaps places or stops instead of attacking; F still attacks.
settings-confirm = Ask before hitting peacefuls
settings-confirm-about = Attacking a peaceful creature asks first.
settings-paranoid-attack = “yes” to hit peacefuls
settings-paranoid-attack-about = That question wants the word yes, not just y.
settings-paranoid-pray = Ask before praying
settings-paranoid-pray-about = Praying asks first: a prayer too soon angers the gods.
settings-paranoid-quit = “yes” to quit without saving
settings-paranoid-quit-about = Quitting without saving, or turning to explore mode, wants the word yes, not just y.
settings-paranoid-remove = Always ask what to take off
settings-paranoid-remove-about = Remove and Take off ask which item even when only one is on.
settings-verbose = Detailed messages
settings-verbose-about = More commentary as you play; off, the messages say only what matters.
settings-gameplay-new = New games start so; a saved game keeps its own.
settings-gameplay-game = This game's own, as it reports them; new games start so too.
settings-gameplay-waiting = The game has not told them yet.
credits-title = Credits and licences
credits-back = Back
credits-hint = ↑↓: choose · PgUp PgDn: scroll · Esc: back
credits-group-credits = Credits
credits-group-licences = Licences
credits-game = renethack
credits-nethack = NetHack
credits-art = Art
credits-sound = Sound
credits-help = The help's Guidebook
credits-translation = The Russian translation
credits-notice = What ships, and its terms
credits-ngpl = NetHack General Public License
credits-godot = Godot Engine
credits-godot-parts = Godot's third-party code
credits-rust = Rust crates
credits-engine-parts = Lua and cJSON
credits-fonts = Fonts
credits-steam = Steamworks
credits-game-text = A client for NetHack 5.0 that draws its dungeon in three dimensions; the game itself is NetHack, played as it is.
credits-game-by = Made by { $author }.
credits-nethack-text = The game the client plays: NetHack 5.0, by the NetHack DevTeam, with renethack's patches and its protocol host.
credits-nethack-licence = NetHack is under the NetHack General Public License (see the licences).
credits-ngpl-missing = The engine's copy of the licence was not found: see NetHack-General-Public-License.txt in the game's legal folder.
credits-lua-missing = Lua's full notice is in Lua-LICENSE.txt in the game's legal folder.
credits-godot-parts-text = Godot Engine { $version } includes the following third-party code.
credits-rust-text = The extension is written in Rust and links these crates; their licence texts follow.
credits-fonts-text = The interface's fonts are under the SIL Open Font License 1.1.
credits-steam-text = This build includes Valve's Steamworks API library (steam_api), redistributed under the Steamworks SDK Access Agreement.
settings-controls = Controls
controls-show = Show
controls-shown-modern = The keyboard: Modern keys
controls-shown-classic = The keyboard: Classic keys
controls-shown-pad = The gamepad
controls-show-about = What the keys and the gamepad do; ↓ goes through the list. A character keeps the keys it was made with.
controls-group-movement = Movement
controls-group-interface = The interface
controls-group-commands = NetHack's commands
controls-group-fixed = Keys that stay as they are
controls-step = A step
controls-zoom-in = Zoom in
controls-zoom-out = Zoom out
controls-count-prefix = A count before a command
controls-slot = Action bar slot { $n }
controls-keypad-moves = The keypad: steps, and rest on its 5
controls-fixed-escape = Esc cancels and goes back, everywhere
controls-fixed-keypad = the keypad's steps
controls-fixed-run = running: Shift and a step
controls-chord = A diagonal step; with Shift, a run
controls-chord-note = No numpad? Press two arrows together to step diagonally.
controls-fixed-arrows = the arrows' steps, beside the letters
controls-fixed-count = a count: the top-row digits, as in NetHack
controls-fixed-alternate = NetHack's own second key for a command
controls-fixed = { $key } stays as it is ({ $what }): press another key, or Esc.
controls-taken = { $key } is the key of "{ $name }". Enter: swap the two · Esc: cancel
controls-press = Press the new key for "{ $name }" · Esc: cancel
controls-edit-about = Enter or a click: another key · Delete: its own key back. Only where NetHack waits for a command: in menus, questions and texts a key is what it is.
controls-own-key = its own key: { $key }
controls-reset-all = Every key back on its own command
controls-reset-all-about = Enter: put every key of these keys back, as NetHack has them.
controls-reset-all-confirm = Enter again to put every key back · Esc: cancel
controls-keypad = the keypad
controls-direction = a direction
controls-left-click = a left click on the map
controls-right-click = a right click on the map
controls-any-key = any key
controls-wheel = the mouse wheel
controls-left-stick = the left stick
controls-right-stick = the right stick
controls-dpad = the d-pad
controls-run = Run
controls-count = A count: twenty searches
controls-inventory = The inventory
controls-palette = The command palette
controls-click = Walk there; take, open or attack what is there
controls-right-click-does = What can be done there
controls-rest = Rest until HP and Pw are full
controls-stop = Stop an order before its next step
controls-overview = The whole known level; again: back to the hero
controls-zoom = Zoom out, zoom in
controls-log = The message log
controls-help = The help: NetHack's Guidebook
controls-pad-walk = Walk in eight directions; in menus, move the choice
controls-pad-cursor = The cursor over the map
controls-pad-a = The cursor cell's action; choose
controls-pad-b = Back, cancel
controls-pad-x = Search; an item's actions
controls-pad-y = The inventory; carry an item to its place
controls-pad-bar-first = Action bar slots 1–4
controls-pad-bar-second = Action bar slots 5–8
controls-pad-bar-page = The bar's other page: slots 9 and 0
controls-pad-bumpers = In menus and the inventory: the page or filter before, after
controls-pad-radial = Held: the ring of actions
controls-pad-fire = Fire
controls-pad-start = The command palette; confirm
controls-pad-history = The message history
controls-pad-sticks = The whole level; the cursor back on the hero

creation-title = New character
creation-name = Name
creation-role = Role
creation-race = Race
creation-gender = Gender
creation-alignment = Alignment
creation-keys = Keys
creation-keys-modern = Modern: walk with the arrows or the keypad
creation-keys-classic = Classic: NetHack's own keys, walk with hjklyubn
creation-keys-tip = Modern: walk with the arrows or the keypad, the letters do the rest (k kicks, j jumps, l loots), n and a number repeat a command, 1–0 use the action bar. Classic: NetHack's own keys (hjklyubn walk), a number before a command repeats it, Alt+1–0 use the action bar.
creation-first-game-key = good for a first game
creation-keys-modern-name = Modern
creation-keys-classic-name = Classic
creation-keys-modern-line = Walk with the arrows or the keypad; the letters do the rest.
creation-keys-classic-line = NetHack's own keys: walk with hjklyubn.
creation-language = Language
creation-random = Random
creation-start = Start
creation-back = Back
creation-continue-instead = Continue { $name } instead
creation-name-taken = A saved game has this name: starting would continue it.
creation-name-placeholder = Empty: the dice choose
creation-name-roll-tip = A name by chance, one that suits the hero
creation-name-bad = A name holds only letters, digits, spaces and ' _ .
# the dice's names: a heroine's, a hero's (at most 31 bytes each)
creation-names-female = Astrid, Aveline, Brenna, Celandine, Dagny, Elowen, Gwendolyn, Hilde, Isolde, Jorunn, Kestrel, Liesel, Maren, Morwenna, Nessa, Odile, Rowena, Sigrun, Tamsin, Una, Vivienne, Wren, Ysolde, Ingrid
creation-names-male = Alaric, Aldric, Bran, Corwin, Duncan, Edric, Fenwick, Gareth, Garrick, Hakon, Ivo, Jasper, Kenrick, Leofric, Merrick, Niall, Osric, Percival, Roderick, Sigurd, Tristan, Ulric, Wystan, Yorick
creation-first-game = Good for a first game
creation-about-random = The dice choose the role when the game starts.
creation-about-arc = Quick and stealthy, with a pick-axe to dig through walls and floors; no great fighter.
creation-about-bar = Strong, in ring mail, with a two-handed sword or an axe, and immune to poison.
creation-about-cav = Tough, with a club and a sling: simple and sturdy, but little gift for magic.
creation-about-hea = A scalpel is a poor weapon, but a healer starts rich and immune to poison, with healing spells and a wand of sleep.
creation-about-kni = Well armoured, with a lance and a pony to ride. A knight's code forbids striking a fleeing foe.
creation-about-mon = Fights bare-handed, fast and hard to hit. Armour and meat go against a monk's training.
creation-about-pri = Sees at a glance whether an item is blessed or cursed. A mace, a shield and clerical spells.
creation-about-rog = Stealthy, and throws daggers several at a time. A stand-up fight is dangerous.
creation-about-ran = Fires arrows in volleys from a bow: keep foes at a distance.
creation-about-sam = Fast and skilled: a katana, a short sword and splint mail.
creation-about-tou = The hardest start: weak, with darts, a camera, scrolls of magic mapping and lots of gold.
creation-about-val = Hardy and stealthy, with a spear and a shield, and resistant to cold: the easiest start.
creation-about-wiz = Knows force bolt from the start, with magic items and a cloak against magic. Frail up close.

end-title = The game is over
end-title-ascended = Ascension
end-title-escaped = Escape from the dungeon
end-new-game = New game
end-to-title = Title
end-quit = Quit
end-hint = ↑↓ PgUp PgDn: scroll · ←→ Tab: choose · Enter: take · Esc: title
end-stone-quit =
    { $female ->
       *[other] gave up
    }
end-ascended =
    { $female ->
        [yes] Ascended to demigoddess-hood
       *[other] Ascended to demigodhood
    }
end-escaped =
    { $female ->
       *[other] Escaped the dungeon alive
    }
end-stat-turns = Turns
end-stat-deepest = Deepest
end-stat-score = Score
end-stat-place = Place
end-stat-gold = Gold
end-stat-level = Level
end-achievements = Achievements
end-achievements-none = None earned in this game.
end-achievement-new = New
end-conduct = Conduct kept
end-conduct-about = Self-imposed challenges: what you never once did all game.
end-conduct-none = None.
end-conduct-foodless = Foodless
end-conduct-vegan =
    { $female ->
       *[other] Vegan
    }
end-conduct-vegetarian =
    { $female ->
       *[other] Vegetarian
    }
end-conduct-atheist =
    { $female ->
       *[other] Atheist
    }
end-conduct-weaponless = Weaponless
end-conduct-pacifist =
    { $female ->
       *[other] Pacifist
    }
end-conduct-illiterate =
    { $female ->
       *[other] Illiterate
    }
end-conduct-polypileless = Never polymorphed an object
end-conduct-polyselfless = Never changed form
end-conduct-wishless = Wishless
end-conduct-artiwishless = No artifact wished for
end-conduct-sokoban = Sokoban's rules kept
end-conduct-petless = Petless
end-chronicle = Chronicle
end-chronicle-none = No events.
end-topten = Top ten
end-possessions = Your possessions
end-topten-none = No scores yet.
end-topten-place = No.
end-topten-points = Points
end-topten-hero = Hero
end-topten-hp = HP

error-title = Something went wrong
error-continue = Continue { $name }
error-to-title = Title
error-quit = Quit

## The prompt line, orders and why they stop

prompt-press-key = Press a key
prompt-press-key-pad = Press { $a }
prompt-more = --More--  (any key)
prompt-more-pad = --More--  ({ $a })
prompt-saving = Saving the game...
prompt-pick-spot = Pick a spot.
prompt-getpos = { $goal }  (Enter or a click: { $verb } · Esc: cancel)
prompt-getpos-pad = { $goal }  ({ $a }: { $verb } · { $b }: cancel)
prompt-getpos-go = go
prompt-getpos-pick = choose
order-walk = Walking
order-walk-pick-up = Walking to pick up
order-stairs-up = Going to the stairs up
order-stairs-down = Going to the stairs down
order-open-door = Going to open the door
order-attack = Going to attack
order-hold = Holding the key
order-rest = Resting until HP and Pw are full
order-line = { $what }  -  any key stops
order-line-pad = { $what }  -  any button stops
stop-arrived = Arrived
stop-done = Done
stop-healed = Rested: HP and Pw are full
stop-one-action = One action in a fight
stop-released = Released
stop-stopped = Stopped
stop-panel = Stopped: a panel opened
stop-focus = Stopped: the window lost focus
stop-question = Stopped: a question
stop-hostile = Stopped: a hostile beside you
stop-hurt = Stopped: you are hurt
stop-hunger = Stopped: hunger changed
stop-condition = Stopped: your condition changed
stop-message = Stopped: { $message }
stop-level = Stopped: another level
stop-blocked = Stopped: the way is blocked
stop-no-path = No known way there
click-unexplored = unexplored
click-off-map = off the map
click-nothing = Nothing to do there ({ $why })

## The action bar

bar-bound = Bound to slot { $key }
bar-cleared = Slot { $key } cleared
bar-restored = Slot { $key } restored
bar-slot-empty-tip = Slot { $key } (empty): drag an item here from the inventory
bar-slot-gone-tip = { $what }: { $item } (not in your pack)
bar-slot-tip = { $what }: { $item }
bar-slot-keys-tip =
    { $tip }
    Key { $key } · NetHack: { $hint } · right-click: clear

## Errors and the game's directory

title-game-saved = Game saved: { $name }.
err-engine-not-built = The game engine is not built.
err-engine-not-built-details =
    { $error }
    Run `make` in the repository, or set RENETHACK_ENGINE_DIR.
err-engine-hangs = The engine stopped responding ({ $secs } s without output).
err-engine-failed = The game engine failed: { $what }
err-game-saved-as = The game is saved; you can continue it as { $name }.
err-killed-by-signal = killed by a signal
err-exit-code = exit code { $code }
err-engine-stopped = The game engine stopped unexpectedly ({ $code }).
err-already-running = renethack is already running with this game directory.
err-already-running-details =
    { $dir }
    Close the other window first, then press Title.
err-playground = Cannot prepare the game directory.
err-engine-does-not-start = The game engine does not start.
err-no-catalog = The game catalog is not available.
err-cannot-restore = This save cannot be restored.
err-cannot-start = Cannot start the game engine.
recover-cannot-look = Cannot look for interrupted games: { $error }
recover-saved = Recovered an interrupted game: { $name }.
recover-lost = An interrupted game ({ $base }) could not be recovered.
recover-failed = Recovering { $base } failed: { $error }

## The HUD

deadly-stone = Turning to stone!
deadly-slime = Turning into slime!
deadly-strangled = Strangled!
deadly-food-poisoning = Food poisoning!
deadly-terminally-ill = Terminally ill!
attr-st = St
attr-dx = Dx
attr-co = Co
attr-in = In
attr-wi = Wi
attr-ch = Ch
hud-attrs-tip = Attributes. Click: your character ({ $key }).
hud-xp-tip = Experience
hud-inventory-tip = Inventory ({ $key })
hud-spells-tip = Spells ({ $key })
hud-character-tip = Character ({ $key })
hud-overview-tip = Dungeon overview ({ $key })
hud-history-tip = Message history ({ $key })
hud-settings-tip = Settings
hud-threat-tip = A hostile in view, out of the frame
hud-ac-tip = Armour class { $ac }
hud-gold-tip = Gold { $gold }
hud-level = Lv { $level }
hud-hit-dice = HD { $hd }
hud-xp = Lv { $level }  ·  { $exp } / { $next }
hud-turn = T { $turn }
orb-hp = Hit points { $value } of { $most }
orb-pw = Power { $value } of { $most }
minimap-tip = The level as far as you know it. Click: walk there.
mode-combat-badge = COMBAT - TURN BY TURN
mode-combat = COMBAT
mode-explore-badge = EXPLORING
mode-explore = EXPLORATION

## The RPG panels: the character sheet (^X)

panel-hint = ↑↓ PgUp PgDn: scroll · Esc, { $key }: close
panel-close-tip = Close (Esc, { $key })
sheet-title = Character
sheet-hit-points = Hit points
sheet-energy = Energy
sheet-armor-class = Armour class
sheet-experience = Experience
sheet-to-level = { $points } to level { $level }
sheet-gold = Gold
sheet-strength = Strength
sheet-dexterity = Dexterity
sheet-constitution = Constitution
sheet-intelligence = Intelligence
sheet-wisdom = Wisdom
sheet-charisma = Charisma
sheet-base = base { $value }
sheet-peak = peak { $value }
sheet-limit = limit { $value }
sheet-innate-limit = innate limit { $value }
sheet-background = Background
sheet-basics = Basics
sheet-characteristics = Characteristics
sheet-status = Status
sheet-attributes = Attributes
sheet-miscellaneous = Miscellaneous

## The RPG panels: the spells (+, Z)

panel-cancel-tip = Cancel (Esc)
spells-title = Spells
spells-cast-title = Cast a spell
spells-swap-title = Swap { $letter } with which spell?
spells-attack = Attack
spells-healing = Healing
spells-divination = Divination
spells-enchantment = Enchantment
spells-clerical = Clerical
spells-escape = Escape
spells-matter = Matter
spells-school-level = { $school } · level { $level }
spells-fail = failure
spells-gone = forgotten
spells-sort = Sort the spells
spells-reorder = Reorder
spells-cast-instead = Cast instead
spells-hint-view = A letter, Enter or a click: cast · Tab: reorder · Esc, { $key }: close
spells-hint-reorder = Pick the spell to move, then the one to swap it with · Tab: cast instead · Esc, { $key }: close
spells-hint-cast = A letter, Enter or a click: cast · Esc, { $key }: close
spells-hint-swap = Pick the spell to swap with · Esc: leave them as they are

## The RPG panels: the skills (#enhance)

skills-title = Skills
skills-title-slots = Skills · { $slots } free
skills-fighting = Fighting
skills-weapons = Weapons
skills-spellcasting = Spellcasting
skills-restricted = Restricted
skills-unskilled = Unskilled
skills-basic = Basic
skills-skilled = Skilled
skills-expert = Expert
skills-master = Master
skills-grand-master = Grand Master
skills-can-advance = advance
skills-later = later
skills-peaked = at its most
skills-hint-advance = A letter, Enter or a click: advance it · Esc, { $key }: close

## The RPG panels: the journal (#chronicle)

journal-title = Journal
journal-events = Events
journal-major-events = Major events
journal-messages = Messages
journal-no-events = No events yet.
journal-no-messages = No messages yet.
journal-hint = ↑↓ PgUp PgDn: the messages · Esc, { $key }: close
journal-hint-esc = ↑↓ PgUp PgDn: the messages · Esc: close

## The RPG panels: the codex (\, #conduct)

codex-title = Codex
codex-discoveries = Discoveries
codex-conduct = Conduct
codex-hint = ↑↓ PgUp PgDn: scroll · Tab: the other tab · Esc, { $key }: close
codex-hint-esc = ↑↓ PgUp PgDn: scroll · Tab: the other tab · Esc: close

## The credits files' own headings and statements (in English the files'
## own words show; these keep the catalogs alike)

credits-md-art = Art credits
credits-md-art-cc0 = Everything listed under “Art credits” is dedicated to the public domain (CC0 1.0, https://creativecommons.org/publicdomain/zero/1.0/). Credit is not required; it is given here gladly.
credits-md-art-changes = Large textures were scaled down and re-encoded, flipbooks converted to PNG, some maps repacked and the armours' metals made brighter; nothing else was changed.
credits-md-ccby = CC-BY
credits-md-ccby-models = The models listed under “CC-BY” are under the Creative Commons Attribution 3.0 license (https://creativecommons.org/licenses/by/3.0/), from Poly Pizza, changed as the English page tells.
credits-md-fonts = Fonts
credits-md-fonts-ofl = The UI's fonts are under the SIL Open Font License 1.1 (not CC0).
credits-md-sound = Sound credits
credits-md-sound-licences = The sounds listed under “Public domain” are in the public domain; those under “Creative Commons Attribution” are licensed as each line says.
credits-md-sound-changes = Changes made: the files were cut, converted, re-encoded and levelled; the loops were crossfaded; the beds listed under Mixes are made of the sources they name.
credits-md-public-domain = Public domain
credits-md-cc-by = Creative Commons Attribution
credits-md-mixes = Mixes
credits-md-help = The help's sources
credits-md-help-about = The in-game help (F1, the HUD's `?` button) shows NetHack's Guidebook in the interface's language, by chapters, with a search.
credits-md-english = English
credits-md-help-english = "A Guide to the Mazes of Menace (Guidebook for NetHack)", original version by Eric S. Raymond, edited and expanded for NetHack 5.0 by Mike Stephenson and others. Part of NetHack, under the NetHack General Public License.
credits-md-russian = Russian
credits-md-help-russian = «Руководство по Грозным Лабиринтам», the Russian translation of the Guidebook (of NetHack 3.4) from <https://github.com/velikodniy/nethack-guide> at commit 576aa83 (2014-09-24). Translated on notabenoid.com (as of 2014-02-01; translator witmolif@gmail.com); edited and typeset by Vadim Velikodniy.
credits-md-help-licence = The repository has no licence file. A translation of NetHack's documentation, it is distributed here under the NetHack General Public License, as the Guidebook is.
credits-md-translation = Credits: the Russian translation
credits-md-memory = Translation memory
credits-md-forms = Russian forms
credits-md-established = Names in established Russian
credits-md-established-about = Where a creature or a thing already has a Russian name, the glossary uses it rather than inventing one; its note names the source.

## The message log

log-title = Messages
log-history-title = Message history
log-history-close = Close  { $key }
bar-undo = Undo
log-count =
    { $n ->
        [one] { $n } message
       *[other] { $n } messages
    }

## Dialogs: questions, menus, text windows, the command palette

dlg-yes = Yes (y)
dlg-no = No (n)
dlg-cancel-q = Cancel (q)
dlg-all-a = All (a)
dlg-yes-pad = Yes
dlg-no-pad = No
dlg-cancel-q-pad = Cancel
dlg-all-a-pad = All
# an answer the question names by a word ("Which ring-finger, Right or
# Left?"): the word, then its key
dlg-named-answer = { $word } ({ $key })
# how an answer's word begins in this language's question
choice-stem = { $word }
dlg-ok = OK
dlg-cancel = Cancel
dlg-count = Count: { $n } — pick an item to take that many
dlg-selected = { $n } selected
dlg-hint-read = ↑↓ PgUp PgDn < >: scroll · Enter or Esc: close
dlg-space-toggle = Space: toggle it
dlg-space-then = then Space toggles it
dlg-hint-any = letter: toggle · ↑↓ mark a row, { $space } · '.' all · '-' none · '@' invert · digits: count · PgUp PgDn < >: scroll · Enter: OK · Esc: cancel
dlg-hint-one = letter or click: pick · ↑↓ mark a row, Enter picks it · PgUp PgDn < >: scroll · Esc: cancel
choice-hint = Esc: cancel
choice-hint-default = { $hint } · Enter: { $answer }
msgmenu-hint-pick = { $letter }: choose · Esc: cancel
msgmenu-hint-close = Enter, Space or Esc: close
show-hint = ↑↓ PgUp PgDn < >: scroll · Enter, Space or Esc: close
text-name-placeholder = a name
text-bytes = { $len } / { $max } bytes
text-hint = Enter: OK · Esc: cancel
palette-title = Extended command
palette-placeholder = type a command
palette-none = no command matches
palette-hint = Enter: run the highlighted command · Tab: complete · ↑↓ PgUp PgDn: choose · Esc: cancel
# the words that find an extended command in the `#` palette beside its
# name and description, a comma between two, else its name (the
# commands' names stay English: they are what one types)
palette-also = { $cmd ->
        [quit] give up
       *[other] { $cmd }
    }
osk-shift = ⇧ Shift
osk-layout = АБВ / ABC
osk-space = Space

## Item actions, filters and bar commands (nh-world's label keys)

item-wield = Wield
item-unwield = Unwield
item-set-alternate = Set as alternate
item-swap-weapons = Swap weapons
item-quiver = Ready in quiver
item-empty-quiver = Empty the quiver
item-fire = Fire
item-throw = Throw
item-apply = Apply
item-wear = Wear
item-take-off = Take off
item-put-on = Put on
item-put-on-left = Put on left hand
item-put-on-right = Put on right hand
item-remove = Remove
item-eat = Eat
item-quaff = Quaff
item-read = Read
item-zap = Zap
item-engrave = Engrave with
item-break = Break
item-drop = Drop
item-drop-some = Drop some…
item-adjust = Adjust letter…
item-split = Split stack…
item-name = Name this item…
item-call = Call this type…
item-dip = Dip into…
item-two-weapon = Two-weapon
item-force = Force a lock
item-rub = Rub on…
item-tip = Tip out
inv-all = All
inv-suggested = Suggested
inv-weapons = Weapons
inv-armor = Armor
inv-accessories = Rings and amulets
inv-tools = Tools
inv-food = Food
inv-potions = Potions
inv-scrolls-and-books = Scrolls and books
inv-wands = Wands
inv-gems-and-other = Gems and other
inv-equipped = Equipped
cmd-search = Search
cmd-rest = Rest until healed
cmd-wait = Wait
cmd-kick = Kick
cmd-pick-up = Pick up
cmd-look-here = Look here
cmd-farlook = Farlook
cmd-travel = Travel
cmd-pray = Pray
cmd-offer = Offer
cmd-chat = Chat
cmd-loot = Loot
cmd-force = Force
cmd-sit = Sit
cmd-turn-undead = Turn undead
cmd-jump = Jump
cmd-ride = Ride
cmd-untrap = Untrap
cmd-open = Open
cmd-close = Close
cmd-pay = Pay
cmd-fire = Fire
cmd-swap = Swap weapons
cmd-two-weapon = Two-weapon
cmd-enhance = Enhance skills
cmd-terrain = Terrain
cmd-overview = Overview
cmd-attributes = Attributes
cmd-discoveries = Discoveries
cmd-cast = Cast a spell
cmd-throw = Throw
cmd-engrave = Engrave
cmd-up = Go up
cmd-down = Go down

## The inventory panel

class-weapon = Weapon
class-armor = Armor
class-ring = Ring
class-amulet = Amulet
class-tool = Tool
class-comestible = Comestible
class-potion = Potion
class-scroll = Scroll
class-spellbook = Spellbook
class-wand = Wand
class-gem = Gem or rock
class-boulder = Boulder or statue
class-iron-ball = Iron ball
class-iron-chain = Iron chain
class-venom = Venom
class-coins = Coins
class-item = Item
# $form: the item's gender and number (m, f, n, pl), for the languages
# whose words agree with it
fact-blessed =
    { $form ->
       *[m] Blessed
    }
fact-uncursed =
    { $form ->
       *[m] Uncursed
    }
fact-cursed =
    { $form ->
       *[m] Cursed
    }
fact-enchantment = Enchantment { $value }
fact-containing = Containing { $what }
fact-containing-items =
    { $n ->
        [one] Containing { $n } item
       *[other] Containing { $n } items
    }
fact-name = Name: { $name }
fact-called = You called this type: { $name }
doll-helmet = Helmet
doll-cloak = Cloak
doll-body = Body armor
doll-shirt = Shirt
doll-gloves = Gloves
doll-boots = Boots
doll-eyes = Eyewear
doll-amulet = Amulet
doll-left-ring = Left ring
doll-right-ring = Right ring
doll-light = Light
doll-leash = Leash
doll-main = Main hand
doll-off = Off hand / shield
doll-alternate = Alternate weapon
doll-quiver = Quiver
doll-main-short = Main
doll-off-short = Off hand
doll-alternate-short = Alternate
doll-quiver-short = Quiver
inv-title = Inventory
inv-choose = Choose
inv-letters = Letters { $used }/52
inv-letters-tip = Inventory letters in use (NetHack keeps 52)
inv-ac = AC { $ac }
inv-gold = Gold { $gold }
inv-confirm = Confirm  Enter
inv-cancel = Cancel  Esc
inv-confirm-pad = Confirm  { $start }
inv-cancel-pad = Cancel  { $b }
inv-close-tip = Close (Esc or i)
inv-suggested-tip = Suggested (?)
inv-search = Search
inv-pack-order = NetHack's pack order
inv-view-grid-tip = The pack as a grid of icons (Ctrl+L)
inv-view-list-tip = The pack as NetHack's list: a line an item, under its class (Ctrl+L)
inv-list-empty = Nothing here.
inv-head-weapons = Weapons
inv-head-armor = Armor
inv-head-rings = Rings
inv-head-amulets = Amulets
inv-head-tools = Tools
inv-head-comestibles = Comestibles
inv-head-potions = Potions
inv-head-scrolls = Scrolls
inv-head-spellbooks = Spellbooks
inv-head-wands = Wands
inv-head-gems = Gems/Stones
inv-head-boulders = Boulders/Statues
inv-head-iron-balls = Iron balls
inv-head-chains = Chains
inv-head-venoms = Venoms
inv-head-coins = Coins
inv-head-other = Other
inv-detail-empty = Select an item to see what you know of it.
inv-count-hint = Type the number or use ← →.  Enter: OK · Esc: cancel
inv-count-hint-pad = ← →: the count · { $a }: OK · { $b }: cancel
inv-count = { $verb }?  { $value } / { $most }
inv-count-how-many = How many
inv-count-drop = Drop how many
inv-count-split = Split off how many
inv-two-actions = { $action }  (2 actions)
inv-cell-select-tip = Click or press { $letter }: { $verb }
inv-cell-menu-tip = Click or press its letter: select · Shift+click: a count
inv-cell-tip = Double-click: { $action } ({ $keys }) · Right-click: actions
inv-doll-tip = { $slot }: { $item }
inv-doll-out-tip = Drag out or double-click: { $action } ({ $keys })
inv-doll-empty-tip = { $slot } (empty) · drag an item here
inv-wielding = Wielding { $item }
inv-empty-handed = Empty handed
inv-pad-filter = { $lb } { $rb }: filter
inv-pad-choose = D-pad: the item · { $a }: this one · { $b }: cancel
inv-pad-select = D-pad: the item · { $a }: choose · { $filter } · { $b }: cancel
inv-pad-menu = D-pad: the item · { $a }: select · { $start }: confirm · { $filter } · { $b }: cancel
inv-pad-carrying = D-pad: where it goes (a doll socket, another item) · { $y }: put it down · { $b }: put it back
inv-pad-browse =
    D-pad: the items and the doll · { $a }: the first action · { $x }: every action
    { $y }: pick up, then { $y } again where it goes (the doll: equip) · { $filter } · { $view }: grid or list · { $b }: close
inv-nothing-suggested = Nothing suggested — any item may be chosen. { $hint }
inv-split-to = Split { $n } off { $from } to which letter?
inv-adjust-to = Adjust { $from } to which letter?
inv-adjust-hint = Type the new letter, or click the item to swap with. Esc: cancel
inv-dip-into = Dip { $from } into what?
inv-dip-hint = Click the item to dip into, or type its letter. Esc: cancel
inv-count-typed = Count { $n }
inv-select-hint = Click an item or press its letter · ? suggested · * all · Esc: cancel
inv-select-hint-count = { $hint } · digits or Shift+click: a count
inv-menu-any-hint =
    Click or letter: select · Shift+click or digits: a count
    '.' all · '-' none · '@' invert · Enter: confirm · Esc: cancel
inv-menu-one-hint = Click or press a letter to choose · Esc: cancel
inv-carrying = Carrying an item
inv-carrying-hint = Move to where it goes (a doll socket, another item) and press Y again · B: put it back
inv-browse-hint =
    Drag to the doll: equip · to the action bar: bind · out of the panel: drop
    Double-click: the first action · Right-click: every action
inv-your-pack = Your pack
inv-show-only = Show only { $what }
inv-class-line = { $class } · letter { $letter }
inv-raw = “{ $text }”
hands-bare = Bare hands
hands-fingers = Fingers
hands-empty-quiver = Nothing (empty the quiver)
hands-nothing = Nothing
fact-recharged =
    { $times ->
        [one] Recharged once
       *[other] Recharged { $times } times
    } · { $charges ->
        [one] { $charges } charge left
       *[other] { $charges } charges left
    }
fact-charges =
    { $n ->
        [one] { $n } charge
       *[other] { $n } charges
    }

## The gamepad: the hint strip, the radial menu

hint-act = Act
hint-search = Search
hint-inventory = Inventory
hint-actions = Actions
hint-wheel = Wheel
hint-fire = Fire
hint-bar = Bar
hint-menu = Menu
hint-pick = Pick
hint-cancel = Cancel
hint-here = Here
hint-toggle = Toggle
hint-confirm = Confirm
hint-page = Page
hint-use = Use
hint-carry = Pick up / put down
hint-filter = Filter
hint-close = Close
hint-choose = Choose
hint-letter = Letter
hint-type = Type
hint-erase = Erase
hint-layout = АБВ / ABC
hint-ok = OK
hint-back = Back
hint-change = Change
hint-section = Section
hint-answer = Answer
hint-run = Run
radial-title = Actions
radial-hint = Point a stick at one · let go in the middle: nothing
radial-let-go = Let go of { $lt } to do it
radial-here = Actions here
radial-pick-up = Pick up
radial-fight = Fight
radial-kick = Kick
radial-rest = Rest
radial-pray = Pray
radial-travel = Travel
radial-save = Save

## The action bar, more

bar-slot-empty = Slot { $key } (empty)

## Pickers of names: a wish, a monster, a class (Russian input)

picker-none = Nothing matches
picker-more = { $n } more: narrow the search
picker-class = Class
picker-class-all = All classes
picker-class-weapon = Weapons
picker-class-armor = Armor
picker-class-ring = Rings
picker-class-amulet = Amulets
picker-class-tool = Tools
picker-class-food = Food
picker-class-potion = Potions
picker-class-scroll = Scrolls
picker-class-spellbook = Spellbooks
picker-class-wand = Wands
picker-class-coin = Coins
picker-class-gem = Gems and stones
picker-class-heavy = Boulders, statues, iron
picker-placeholder-wish = For example: blessed +2 long sword
picker-placeholder-monster = A monster's name
picker-placeholder-class = A class, or one of its monsters
picker-placeholder-write = What to write
picker-hint-wish = Type the wish: a count, blessed or cursed, +enchantment, the thing · Enter: wish · ↑↓: choose · Esc: cancel
picker-hint = Enter: choose · ↑↓: move · Esc: cancel
picker-wish = Wish
picker-choose = Choose
picker-manual = Type in English
wish-count = Count
wish-ench = Enchantment
wish-buc-any = Blessing: any
wish-buc-blessed = Blessed
wish-buc-uncursed = Uncursed
wish-buc-cursed = Cursed
wish-shown = You wish for: { $wish }
wish-pick = Choose the thing in the list
text-latin = Engraved in Latin letters: { $text }
hint-wish = Wish
hint-count = Count
hint-blessing = Blessing
hint-enchantment = Enchantment
hint-class = Class
hint-english = In English

## Achievements

title-achievements = Achievements
hud-achievements-tip = Achievements
achievements-title = Achievements
achievements-progress = { $earned } of { $total } earned
achievements-unlocked = Achievement unlocked
achievements-hidden-name = Hidden achievement
achievements-hidden-desc = Keep playing to find out what it is.
achievements-locked = Not earned yet
achievements-earned = Earned by { $character } on turn { $turn }, { $date }
achievements-hint = Arrows: choose · Esc: back
achievements-back = Back

achievement-bell-name = Ring My Bell
achievement-bell-desc = Take the Bell of Opening from your quest nemesis.
achievement-gehennom-name = Abandon All Hope
achievement-gehennom-desc = Enter Gehennom through the Valley of the Dead.
achievement-candelabrum-name = Seven Candles
achievement-candelabrum-desc = Take the Candelabrum of Invocation from Vlad's Tower.
achievement-book-of-the-dead-name = Required Reading
achievement-book-of-the-dead-desc = Take the Book of the Dead from the Wizard of Yendor.
achievement-invocation-name = The Gate Opens
achievement-invocation-desc = Perform the invocation at the vibrating square.
achievement-amulet-name = The Prize
achievement-amulet-desc = Take the Amulet of Yendor from the high priest of Moloch.
achievement-planes-name = Beyond the Dungeon
achievement-planes-desc = Carry the Amulet up into the Elemental Planes.
achievement-astral-name = Among the Stars
achievement-astral-desc = Reach the Astral Plane.
achievement-ascension-name = Demigod
achievement-ascension-desc = Offer the Amulet of Yendor to your god and ascend.
achievement-mines-prize-name = Feeling Lucky
achievement-mines-prize-desc = Find the luckstone hidden at Mines' End.
achievement-sokoban-prize-name = Puzzle Solved
achievement-sokoban-prize-desc = Claim the prize at the top of Sokoban.
achievement-medusa-name = Stone Cold
achievement-medusa-desc = Kill Medusa.
achievement-blind-name = In the Dark
achievement-blind-desc = Finish a game blind from the first turn to the last.
achievement-nudist-name = Nothing to Wear
achievement-nudist-desc = Finish a game without ever wearing armour.
achievement-mines-name = Gnomish Mines
achievement-mines-desc = Enter the Gnomish Mines.
achievement-minetown-name = Minetown
achievement-minetown-desc = Reach Minetown.
achievement-shop-name = Customer
achievement-shop-desc = Enter a shop.
achievement-temple-name = Sanctuary
achievement-temple-desc = Enter a temple.
achievement-oracle-name = Words of Wisdom
achievement-oracle-desc = Consult the Oracle.
achievement-novel-name = A Good Read
achievement-novel-desc = Read a passage from a Discworld novel.
achievement-sokoban-name = Sokoban
achievement-sokoban-desc = Enter Sokoban.
achievement-big-room-name = The Big Room
achievement-big-room-desc = Enter the Big Room.
achievement-rank-1-name = Rising
achievement-rank-1-desc = Reach experience level 3 and your role's next title.
achievement-rank-2-name = Seasoned
achievement-rank-2-desc = Reach experience level 6 and your role's next title.
achievement-rank-3-name = Veteran
achievement-rank-3-desc = Reach experience level 10 and your role's next title.
achievement-rank-4-name = Hardened
achievement-rank-4-desc = Reach experience level 14 and your role's next title.
achievement-rank-5-name = Renowned
achievement-rank-5-desc = Reach experience level 18 and your role's next title.
achievement-rank-6-name = Famed
achievement-rank-6-desc = Reach experience level 22 and your role's next title.
achievement-rank-7-name = Legendary
achievement-rank-7-desc = Reach experience level 26 and your role's next title.
achievement-rank-8-name = Peerless
achievement-rank-8-desc = Reach experience level 30 and your role's next title.
achievement-tune-name = Five Notes
achievement-tune-desc = Learn the tune that opens the castle's drawbridge.
achievement-quest-called-name = The Call
achievement-quest-called-desc = Be called to your Quest by your leader.
achievement-quest-done-name = Quest Fulfilled
achievement-quest-done-desc = Bring your quest artifact back to your leader.
achievement-crowned-name = Crowned
achievement-crowned-desc = Be crowned by your god.
achievement-wizard-name = Wizard Slayer
achievement-wizard-desc = Kill the Wizard of Yendor.
achievement-drawbridge-name = Open Sesame
achievement-drawbridge-desc = Open the castle's drawbridge.
achievement-vibrating-square-name = Good Vibrations
achievement-vibrating-square-desc = Find the vibrating square.
achievement-major-oracle-name = A Major Consultation
achievement-major-oracle-desc = Pay the Oracle for a major consultation.
achievement-amulet-wish-name = Wish Upon the Amulet
achievement-amulet-wish-desc = Gain a wish from the Amulet of Yendor.
achievement-depth-10-name = Down Below
achievement-depth-10-desc = Reach dungeon level 10.
achievement-depth-20-name = Deep Delver
achievement-depth-20-desc = Reach dungeon level 20.
achievement-depth-30-name = Underworld
achievement-depth-30-desc = Reach dungeon level 30.
achievement-depth-40-name = Depths of Despair
achievement-depth-40-desc = Reach dungeon level 40.
achievement-ascend-arc-name = Ascended Archeologist
achievement-ascend-arc-desc = Ascend as an archeologist.
achievement-ascend-bar-name = Ascended Barbarian
achievement-ascend-bar-desc = Ascend as a barbarian.
achievement-ascend-cav-name = Ascended Cave Dweller
achievement-ascend-cav-desc = Ascend as a cave dweller.
achievement-ascend-hea-name = Ascended Healer
achievement-ascend-hea-desc = Ascend as a healer.
achievement-ascend-kni-name = Ascended Knight
achievement-ascend-kni-desc = Ascend as a knight.
achievement-ascend-mon-name = Ascended Monk
achievement-ascend-mon-desc = Ascend as a monk.
achievement-ascend-pri-name = Ascended Cleric
achievement-ascend-pri-desc = Ascend as a priest or priestess.
achievement-ascend-rog-name = Ascended Rogue
achievement-ascend-rog-desc = Ascend as a rogue.
achievement-ascend-ran-name = Ascended Ranger
achievement-ascend-ran-desc = Ascend as a ranger.
achievement-ascend-sam-name = Ascended Samurai
achievement-ascend-sam-desc = Ascend as a samurai.
achievement-ascend-tou-name = Ascended Tourist
achievement-ascend-tou-desc = Ascend as a tourist.
achievement-ascend-val-name = Ascended Valkyrie
achievement-ascend-val-desc = Ascend as a valkyrie.
achievement-ascend-wiz-name = Ascended Wizard
achievement-ascend-wiz-desc = Ascend as a wizard.
achievement-ascend-vegan-name = Vegan Ascension
achievement-ascend-vegan-desc = Ascend without eating any animal or animal product.
achievement-ascend-vegetarian-name = Vegetarian Ascension
achievement-ascend-vegetarian-desc = Ascend without eating any animal.
achievement-ascend-foodless-name = Foodless Ascension
achievement-ascend-foodless-desc = Ascend without eating anything at all.
achievement-ascend-atheist-name = Atheist Ascension
achievement-ascend-atheist-desc = Ascend without praying, using an altar or asking a priest.
achievement-ascend-weaponless-name = Weaponless Ascension
achievement-ascend-weaponless-desc = Ascend without hitting anything with a wielded weapon.
achievement-ascend-pacifist-name = Pacifist Ascension
achievement-ascend-pacifist-desc = Ascend without killing a single monster yourself.
achievement-ascend-illiterate-name = Illiterate Ascension
achievement-ascend-illiterate-desc = Ascend without reading anything.
achievement-ascend-polypileless-name = Polypileless Ascension
achievement-ascend-polypileless-desc = Ascend without polymorphing an object.
achievement-ascend-polyselfless-name = Polyselfless Ascension
achievement-ascend-polyselfless-desc = Ascend without changing your form.
achievement-ascend-wishless-name = Wishless Ascension
achievement-ascend-wishless-desc = Ascend without wishing for anything.
achievement-ascend-artiwishless-name = Artifact Wishless Ascension
achievement-ascend-artiwishless-desc = Ascend without wishing for an artifact.
achievement-ascend-petless-name = Petless Ascension
achievement-ascend-petless-desc = Ascend without ever having a pet.
achievement-sokoban-purist-name = Sokoban Purist
achievement-sokoban-purist-desc = Claim the Sokoban prize without breaking Sokoban's rules.

## The help: NetHack's Guidebook

hud-help-tip = Help: the guide and the Guidebook ({ $key })
help-title = Help — { $book }
help-close-tip = Close (Esc, { $key })
help-search-placeholder = Search the Guidebook
help-nothing = Nothing found
help-hint = ↑↓: chapter · PgUp PgDn: scroll · Esc: close
help-chord = [b]No numpad?[/b] Press two arrows together to step diagonally: ↑ and ← north-west, ↑ and → north-east, ↓ and ← south-west, ↓ and → south-east. Held, the hero keeps going; with Shift, runs. A direction asked, and the cursor on the map, take them too.
guide-card-kicker = A hint
guide-card-close = Space · Esc · a click: close
guide-quest-title = Your quest
guide-quest = Far below lies the Amulet of Yendor. Go down through the Dungeons of Doom, find it, bring it back up, and offer it to your god.
guide-pray-title = In trouble? Pray
guide-pray = Weak with hunger, or few hit points left: pray (#pray). Your god may help, but not if you prayed too recently: let about a thousand turns pass between prayers.
guide-pray-pad = Weak with hunger, or few hit points left: pray (hold { $lt }, then Pray). Your god may help, but not if you prayed too recently: let about a thousand turns pass between prayers.
guide-monster-title = A creature
guide-monster = Walk into it to fight, or click it. A peaceful one asks before you attack it; { $fight } and a direction fights without moving.
guide-monster-pad = Walk into it to fight. A peaceful one asks before you attack it. To fight without moving: hold { $lt }, pick Fight, then a direction.
guide-wounded-title = Wounded
guide-wounded = Hit points come back slowly by themselves. Rest ({ $rest }) or search ({ $search }) when nothing is near.
guide-wounded-pad = Hit points come back slowly by themselves. Rest (hold { $lt }, then Rest) or search ({ $x }) when nothing is near.
guide-hungry-title = Hungry
guide-hungry = Eat something soon ({ $eat }, or Eat in an item's menu), or you will grow weak.
guide-hungry-pad = Eat something soon (Eat in an item's menu: { $y } opens your pack), or you will grow weak.
guide-shop-title = A shop
guide-shop = Pick a thing up to hear its price, and pay with { $pay }. Leave with goods you haven't paid for, and the shopkeeper attacks.
guide-shop-pad = Pick a thing up to hear its price, and pay: the game's menu ({ $start }), All commands, then “Pay your shopping bill”, the first row while you carry goods not paid for. Leave with them, and the shopkeeper attacks.
guide-item-here-title = Something here
guide-item-here = Pick it up with { $pickup }, or click yourself.
guide-item-here-pad = Pick it up: hold { $lt }, then Pick up.
guide-unidentified-title = Something unknown
guide-unidentified = You don't know what this is yet: its name comes when you use it, or when you identify it.
guide-level-up-title = A new level
guide-level-up = More hit points, and a better chance to hit. Your rank shows under your name.
guide-enhance-title = A skill can improve
guide-enhance = #enhance opens your skills; the ones ready to advance are marked.
guide-enhance-pad = Your skills are in the game's menu ({ $start }); the ones ready to advance are marked.
guide-pet-title = Your pet
guide-pet = It fights at your side. Keep it next to you on the stairs to bring it along. It steps only reluctantly onto cursed things.
guide-trap-title = A trap
guide-trap = Walk around it. A known trap asks before you step onto it.
guide-stairs-down-title = Stairs down
guide-stairs-down = Stand on them and press { $down }, or click them, to go deeper. The Amulet lies far below.
guide-stairs-down-pad = Stand on them, press { $a } and pick going down; or point the cursor at them with the right stick and press { $a }. The Amulet lies far below.
guide-door-title = A door
guide-door = Walk into a closed door to open it. A locked one: kick it ({ $kick }), or unlock it with a key.
guide-door-pad = Walk into a closed door to open it. A locked one: kick it (hold { $lt }, then Kick), or unlock it with a key.
guide-fountain-title = A fountain
guide-fountain = Drinking from it ({ $quaff }) or dipping into it is a gamble: good things are rare.
guide-fountain-pad = Drinking from it (stand on it and press { $a }) or dipping into it is a gamble: good things are rare.
guide-altar-title = An altar
guide-altar = Drop things on it to see whether they are blessed or cursed.
help-guide = Guide
help-guide-about = This game's own guide: “Your quest” on its screen, moving and fighting, then the cards you have met, to read again (Settings → Interface → Hints turns the cards on or off). NetHack's own Guidebook follows; its chapters marked “terminal” describe the screen of NetHack's terminal version.
# a mark after the Guidebook's chapters on the screen in the help's list
help-terminal-tag = · terminal
# the first words of those chapters; $page: the guide's first page
help-terminal-note = This chapter describes the screen of NetHack's terminal version: a map drawn in letters, the status lines at the bottom, the messages at the top. This game's screen is another: “{ $page }”, the first page of this help, shows it.
quest-goal-more = The way is long, and the dungeon is different in every game. These pages show you around; the help's Guide keeps them, and the cards that come up as you play.
quest-screen-title = Your screen
quest-screen = Around the dungeon, the game shows you the parts named here.
# $banner: mode-combat, $badge: mode-combat-badge
quest-screen-combat = A fight puts up the { $banner } banner, and the mode turns to { $badge }: from then on, every action waits for you.
quest-screen-overview = { $overview }: the whole level as you know it; again, back to the hero.
quest-screen-overview-pad = { $l3 } (the left stick pressed): the whole level as you know it; again, back to the hero.
quest-screen-tips = Rest the mouse on any of them for its tip.
quest-tour-portrait = You: your rank, alignment and attributes
quest-tour-hp = Hit points: at none, you die
quest-tour-pw = Power, for spells
quest-tour-minimap = The level as you know it
quest-tour-mode = The mode: exploring, or a fight
quest-tour-buttons = Panels: pack, spells, hero, dungeon…
quest-tour-buttons-pad = Panels, all in { $start }'s menu too
quest-tour-bar = Actions: { $first }…{ $last }, or a click
quest-tour-bar-pad = Actions: { $lb }, { $rb } or both, with { $a } { $b } { $x } { $y }
quest-tour-log = What happens, in words
quest-moves-title = Moving and fighting
quest-moves-keys = Walk with { $steps }. No numpad? Press two arrows together to step diagonally.
quest-steps-modern = the arrows, Home, PgUp, End and PgDn, or the keypad
quest-steps-classic = h j k l y u b n, or the arrows
quest-moves-fight = Walk into a creature to fight it. { $fight } and a direction fights without moving.
quest-moves-click = Click a place to walk there, a creature to fight it, a thing to pick it up; right-click for what can be done there.
quest-moves-pad = With a gamepad: the left stick walks, { $a } acts, { $lt } held opens the ring of actions.
quest-moves-pad-walk = The left stick or the d-pad walks; held, the hero keeps going.
quest-moves-pad-fight = Walk into a creature to fight it. { $rt } fires; { $lt } held opens the ring of actions: pick up, fight in place, kick, rest, pray, travel.
quest-moves-pad-cursor = The right stick moves a cursor over the map; { $a } does what can be done there: walk, fight, pick up, and on yourself what can be done here.
quest-moves-pad-more = { $y }: your pack · { $x }: search · { $back }: the messages · { $start }: the game's menu, with every panel and command
quest-moves-more = { $inventory }: your pack · { $palette }: every command · { $help }: the help
quest-next = Next
quest-back = Back
quest-skip = Skip
quest-play = Play
quest-hint = Any key or a click: next · ←: back · Esc: skip

## The game's menu (Esc at the command prompt)

game-menu-title = Game
game-menu-resume = Back to the game
game-menu-settings = Settings
game-menu-save = Save and leave
game-menu-give-up = Give up
game-menu-give-up-about = Giving up ends this game for good: the hero dies, and nothing is saved.
game-menu-help = Help
game-menu-commands = All commands
game-menu-hint = ↑↓ ←→: choose · Enter: take · Esc: back to the game
quit-warning = The hero dies here and the game ends for good: it is not saved and cannot be continued.
palette-give-up = Give up: end this game for good, without saving
hint-chapter = Chapter
hint-scroll = Scroll
hint-view = Grid or list

## The status: the level, experience and score

hud-dlvl = Dlvl { $level }
hud-tutorial-level = Tutorial { $level }
# the Quest's levels, as the status line says them
hud-home-level = Home { $level }
hud-exp = Exp { $exp }
hud-score = Score { $score }

## The engine's pictures and tables, laid out by the client: the
## tombstone, #vanquished, #genocided, #overview

rip-rest-in-peace =
    REST
    IN
    PEACE
rip-gold = { $gold } Au
vanquished-title = Vanquished creatures
vanquished-rider = Rider
vanquished-total =
    { $count ->
        [one] { $count } creature vanquished
       *[other] { $count } creatures vanquished
    }
genocided-title = Genocided species
genocided-title-extinct = Extinct species
genocided-title-both = Genocided or extinct species
genocided-extinct = extinct
genocided-total = { $count } species genocided
extinct-total = { $count } species extinct
overview-title = Dungeon overview
overview-levels = levels { $from }–{ $to }
overview-levels-up = levels { $from } up to { $to }
overview-level = Level { $level }
overview-astral = Astral Plane
overview-plane-earth = Plane of Earth
overview-plane-air = Plane of Air
overview-plane-fire = Plane of Fire
overview-plane-water = Plane of Water
overview-here = You are here
overview-left-from = You left from here
overview-were = You were here
overview-note = “{ $note }”
overview-shops =
    { $seen ->
        [two] 2 shops
       *[many] many shops
    }
overview-temples =
    { $seen ->
        [one] temple
        [two] 2 temples
       *[many] many temples
    }
overview-temples-to =
    { $seen ->
        [one] temple to { $god }
        [two] 2 temples to { $god }
       *[many] many temples to { $god }
    }
overview-altars =
    { $seen ->
        [one] altar
        [two] 2 altars
       *[many] many altars
    }
overview-altars-to =
    { $seen ->
        [one] altar to { $god }
        [two] 2 altars to { $god }
       *[many] many altars to { $god }
    }
overview-thrones =
    { $seen ->
        [one] throne
        [two] 2 thrones
       *[many] many thrones
    }
overview-fountains =
    { $seen ->
        [one] fountain
        [two] 2 fountains
       *[many] many fountains
    }
overview-sinks =
    { $seen ->
        [one] sink
        [two] 2 sinks
       *[many] many sinks
    }
overview-graves =
    { $seen ->
        [one] grave
        [two] 2 graves
       *[many] many graves
    }
overview-trees =
    { $seen ->
        [one] tree
        [two] 2 trees
       *[many] many trees
    }
overview-oracle = Oracle of Delphi
overview-sokoban-solved = Solved
overview-sokoban-unsolved = Unsolved
overview-bigroom = A very big room
overview-rogue = A primitive area
overview-home = Home
overview-home-lost = Home (no way back…)
overview-quest-done = Completed quest for { $leader }
overview-quest-given = Given quest by { $leader }
overview-summoned = Summoned by { $leader }
overview-ludios = Fort Ludios
overview-castle = The castle
overview-castle-notes = The castle: play { $notes } to open or close the drawbridge
overview-castle-tune = The castle: play the 5-note tune to open or close the drawbridge
overview-valley = Valley of the Dead
overview-gateway = Gateway to Moloch's Sanctum
overview-sanctum = Moloch's Sanctum
overview-stairs-up = Stairs up to { $place }
overview-stairs-down = Stairs down to { $place }
overview-one-way-up = One-way stairs up to { $place }
overview-one-way-down = One-way stairs down to { $place }
overview-portal = Portal to { $place }
overview-sealed-portal = Sealed portal to { $place }
overview-connection = Connection to { $place }
overview-unknown-way = A way to { $place }
overview-branch-level = { $branch }, level { $level }
overview-resting = Final resting place for
overview-resting-you = Final resting place for
overview-dead-you = you, { $how }
overview-dead = { $who }, { $how }

# the prologue: NetHack's legacy told over scenes before a game
prologue-hint = Any key or A: next  ·  Esc or B: skip
prologue-hint-pad = { $a }: next  ·  { $b }: skip
title-prologue = Prologue
settings-prologue = Prologue
settings-prologue-about = NetHack's own story of the Amulet, told over scenes before a game.
settings-prologue-first = Before the first game
settings-prologue-every = Before every new game
settings-prologue-never = Never
