# renethack: the client's own interface, in English (the source; ru.ftl
# has the same keys). The engine's words (messages, names, menus) are not
# here: nh-i18n translates those.

## The title, settings, character creation, the end of a game, errors

title-subtitle = NetHack 5.0
title-new-game = New game
title-continue = Continue: { $name } ({ $time })
title-settings = Settings
title-quit = Quit

settings-title = Settings
settings-language = Language
settings-back = Back

creation-title = New character
creation-name = Name
creation-role = Role
creation-race = Race
creation-gender = Gender
creation-alignment = Alignment
creation-keys = Keys
creation-keys-modern = Modern: arrows and keypad move, n counts
creation-keys-classic = Classic: hjklyubn move, Alt+digits count
creation-keys-tip = Both keep the action bar on 1-0. Modern plays with number_pad (k kicks, j jumps, l loots); Classic with NetHack's vi-keys.
creation-language = Language
creation-random = Random
creation-start = Start
creation-back = Back
creation-continue-instead = Continue { $name } instead
creation-name-taken = A saved game has this name: starting would continue it.

end-title = The game is over
end-new-game = New game
end-to-title = Title

error-title = Something went wrong
error-continue = Continue { $name }
error-to-title = Title
error-quit = Quit

## The prompt line, orders and why they stop

prompt-press-key = Press a key
prompt-more = --More--  (any key)
prompt-saving = Saving the game...
prompt-pick-spot = Pick a spot.
prompt-getpos = { $goal }  (. , ; : pick, ? help, Esc cancel)
order-walk = Walking
order-walk-pick-up = Walking to pick up
order-stairs-up = Going to the stairs up
order-stairs-down = Going to the stairs down
order-open-door = Going to open the door
order-attack = Going to attack
order-search = Searching, { $left } more
order-wait = Waiting, { $left } more
order-walk-steps =
    Walking, { $left } more { $left ->
        [one] step
       *[other] steps
    }
order-hold = Holding the key
order-rest = Resting until HP and Pw are full
order-line = { $what }  -  any key stops
stop-arrived = Arrived
stop-done = Done
stop-healed = Rested: HP and Pw are full
stop-one-action = One action in a fight
stop-released = Released
stop-stopped = Stopped
stop-panel = Stopped: a panel opened
stop-focus = Stopped: the window lost focus
stop-question = Stopped: a question
stop-hostile = Stopped: a hostile in view
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
hud-attrs-tip = Attributes. Click: your character (^X).
hud-xp-tip = Experience
hud-inventory-tip = Inventory (i)
hud-spells-tip = Spells (+)
hud-character-tip = Character (^X)
hud-overview-tip = Dungeon overview (^O)
hud-history-tip = Message history (F9)
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

## The message log

log-title = Messages
log-history-title = Message history
log-history-close = Close  F9
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
osk-shift = ⇧ Shift
osk-layout = АБВ / ABC
osk-space = Space
osk-hint = D-pad: a key · A: type · B: erase · Y: АБВ/ABC · Start: OK

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
fact-blessed = Blessed
fact-uncursed = Uncursed
fact-cursed = Cursed
fact-enchantment = Enchantment { $value }
fact-containing = Containing { $what }
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
inv-close-tip = Close (Esc or i)
inv-suggested-tip = Suggested (?)
inv-search = Search
inv-pack-order = NetHack's pack order
inv-detail-empty = Select an item to see what you know of it.
inv-count-hint = Type the number or use ← →.  Enter: OK · Esc: cancel
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
    { $y }: pick up, then { $y } again where it goes (the doll: equip) · { $filter } · { $b }: close
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
hint-fire = Fire
hint-bar = Bar
hint-commands = Commands
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
hint-type = Type
hint-erase = Erase
hint-layout = АБВ / ABC
hint-ok = OK
hint-back = Back
radial-title = Actions
radial-hint = Point a stick at one · let go in the middle: nothing
radial-let-go = Let go of LT to do it
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
