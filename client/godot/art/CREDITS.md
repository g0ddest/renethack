# Art credits

Everything under `cc0/` is dedicated to the public domain (CC0 1.0,
https://creativecommons.org/publicdomain/zero/1.0/). Credit is not required;
it is given here gladly. `tools/fetch_art.py` fetches these files again from
the recipe in `sources.json` and checks the downloads against `art.lock.json`.

| Where | What | Author | Source |
|---|---|---|---|
| `cc0/polyhaven/textures/` | stone tiles, castle brick, dirt floor, rock walls, old planks, rusty metal, marble, stone walls, castle wall, monastery and rock-tile floors, volcanic rock tiles (1K) | Poly Haven artists | https://polyhaven.com/textures |
| `cc0/polyhaven/models/` | street rat, boulder, dead tree trunk, gothic statue; estoc, dagger, mace, war hammer, kite shield, three axes, oil lamp, lantern, pick; barrels, crates, treasure chest, stone fire pit (1K textures) | Poly Haven artists | https://polyhaven.com/models |
| `cc0/quaternius/outfits/` | Modular Character Outfits – Fantasy (Standard): peasant, ranger | Quaternius | https://quaternius.itch.io/modular-character-outfits-fantasy |
| `cc0/quaternius/animations/` | Universal Animation Library (Standard) | Quaternius | https://quaternius.itch.io/universal-animation-library |
| `cc0/quaternius/animations/UAL2_Standard.glb` | Universal Animation Library 2 (Standard) | Quaternius | https://quaternius.itch.io/universal-animation-library-2 |
| `cc0/quaternius/weapons/` | Medieval Weapons Pack: bows, arrow, spear | Quaternius | https://quaternius.itch.io/lowpoly-medieval-weapons |
| `cc0/quaternius/animals/Wolf.glb`, `Husky.glb` | Ultimate Animated Animal Pack: wolf, husky | Quaternius | https://quaternius.com/packs/ultimateanimatedanimals.html (fetched from the Poly Pizza mirror: https://poly.pizza/m/P1gU3Qkr9r, https://poly.pizza/m/wcWiuEqwzq) |
| `cc0/quaternius/bestiary/` | Bestiary – Dungeon Monsters Kit (Standard): imp, puglin | Quaternius | https://quaternius.itch.io/bestiary-dungeon-monsters-kit |
| `cc0/quaternius/props/` | Fantasy Props MegaKit (Standard), a selection | Quaternius | https://quaternius.itch.io/fantasy-props-megakit |
| `cc0/quaternius/animals/` | Farm Animals Animated: pug, horse, cow, pig, sheep | Quaternius | https://quaternius.itch.io/lowpoly-animated-animals |
| `cc0/sigilsvault/dungeon/` | Modular Dungeon Kit v1.0: pieces and props (GLB) | Kevin Barany (SigilsVault) | https://sigilsvault.itch.io/modular-dungeon-kit-v10 |
| `cc0/unity-labs/flipbooks/` | VFX flipbooks: Flame02, Flame03, FireBall01–04, WispySmoke01, CandleSmoke01, Explosion02HD | Unity Technologies (Unity Labs Paris) | https://unity.com/blog/engine-platform/free-vfx-image-sequences-flipbooks |
| `cc0/kenney/particles/` | Particle Pack (transparent PNGs) | Kenney | https://kenney.nl/assets/particle-pack |
| `cc0/icons/flare/armor.png` | Armor Icons by Equipment Slot | Clint Bellanger, Blarumyrran, crowline, Justin Nichol | https://opengameart.org/content/armor-icons-by-equipment-slot |
| `cc0/icons/flare/weapons-2/` | Flare weapon icons 2 | Clint Bellanger | https://opengameart.org/content/flare-weapon-icons-2 |
| `cc0/icons/flare/osare/` | OSARE weapon icons | Blarumyrran | https://opengameart.org/content/osare-weapon-icons |

Textures larger than 1024 px (512 px for the SigilsVault kit) were scaled
down and re-encoded, and the flipbooks were converted from TGA to PNG at full
size; nothing else was changed.

Godot writes an `.import` file next to every asset when it imports the
project (committed, as usual for Godot projects: they hold the import
settings, e.g. mipmaps and GPU compression for the textures) and extracts
the textures embedded in `bestiary/*.glb` and `sigilsvault/dungeon/*/*.glb`
next to them (`Imp_T_Imp_*.jpg`, `Puglin_T_Puglin_*.jpg`, `<piece>_TrimSheet_*.jpg`…). `manifest.json` says which
model, material and animation draws each monster, object and map feature.

## Fonts

The UI's fonts are under the SIL Open Font License 1.1 (not CC0); each
family's `OFL.txt` is next to its files in `client/godot/fonts/`.
`tools/fetch_art.py` fetches them again from the google/fonts repository at
the commit named in `sources.json` and checks them against `art.lock.json`.

| Where | What | Author | Source |
|---|---|---|---|
| `fonts/cormorant-sc/` | Cormorant SC SemiBold, Bold (titles) | Christian Thalmann (Catharsis Fonts) | https://github.com/google/fonts/tree/main/ofl/cormorantsc |
| `fonts/alegreya-sans/` | Alegreya Sans Medium, Bold, Black (body, numbers) | Juan Pablo del Peral (Huerta Tipográfica) | https://github.com/google/fonts/tree/main/ofl/alegreyasans |
| `fonts/alegreya-sans-sc/` | Alegreya Sans SC Bold (key labels, small caps) | Juan Pablo del Peral (Huerta Tipográfica) | https://github.com/google/fonts/tree/main/ofl/alegreyasanssc |
| `fonts/pt-mono/` | PT Mono (the engine's own text: menus, text windows) | ParaType | https://github.com/google/fonts/tree/main/ofl/ptmono |
