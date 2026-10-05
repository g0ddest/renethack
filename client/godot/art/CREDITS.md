# Art credits

Everything under `cc0/` is dedicated to the public domain (CC0 1.0,
https://creativecommons.org/publicdomain/zero/1.0/). Credit is not required;
it is given here gladly. `tools/fetch_art.py` fetches these files again from
the recipe in `sources.json` and checks the downloads against `art.lock.json`.

| Where | What | Author | Source |
|---|---|---|---|
| `cc0/polyhaven/textures/` | stone tiles, castle brick, dirt floor, rock walls, old planks, rusty metal, marble, stone walls, castle wall, monastery and rock-tile floors, volcanic rock tiles (1K) | Poly Haven artists | https://polyhaven.com/textures |
| `cc0/polyhaven/models/` | street rat, boulder, dead tree trunk, gothic statue; estoc, dagger, mace, war hammer, kite shield, three axes, oil lamp, lantern, pick; barrels, crates, treasure chest, stone fire pit; large castle door, large iron gate, lantern chandelier, wooden candlestick, brass candleholders (1K textures) | Poly Haven artists | https://polyhaven.com/models |
| `cc0/quaternius/outfits/` | Modular Character Outfits – Fantasy (Standard): peasant, ranger | Quaternius | https://quaternius.itch.io/modular-character-outfits-fantasy |
| `cc0/quaternius/animations/` | Universal Animation Library (Standard) | Quaternius | https://quaternius.itch.io/universal-animation-library |
| `cc0/quaternius/animations/UAL2_Standard.glb` | Universal Animation Library 2 (Standard) | Quaternius | https://quaternius.itch.io/universal-animation-library-2 |
| `cc0/quaternius/weapons/` | Medieval Weapons Pack: bows, arrow, spear | Quaternius | https://quaternius.itch.io/lowpoly-medieval-weapons |
| `cc0/quaternius/animals/Wolf.glb`, `Husky.glb`, `Horse.glb`, `WhiteHorse.glb`, `Cow.glb`, `Bull.glb` | Ultimate Animated Animal Pack: wolf, husky, horse, white horse, cow, bull | Quaternius | https://quaternius.com/packs/ultimateanimatedanimals.html (fetched from the Poly Pizza mirror: https://poly.pizza/m/P1gU3Qkr9r, https://poly.pizza/m/wcWiuEqwzq, https://poly.pizza/m/qvTrSG9pZF, https://poly.pizza/m/bEdE4rmZy9, https://poly.pizza/m/26zM1outCr, https://poly.pizza/m/a8PIIYwF7r) |
| `cc0/quaternius/base/` | Universal Base Characters (Standard): the base heads (face, eyes, eyebrows), hairstyles and beards | Quaternius | https://quaternius.itch.io/universal-base-characters |
| `cc0/quaternius/bestiary/` | Bestiary – Dungeon Monsters Kit (Standard): imp, puglin | Quaternius | https://quaternius.itch.io/bestiary-dungeon-monsters-kit |
| `cc0/quaternius/props/` | Fantasy Props MegaKit (Standard), a selection | Quaternius | https://quaternius.itch.io/fantasy-props-megakit |
| `cc0/quaternius/monsters/Spider.glb`, `Snake.glb`, `Bat.glb`, `Dragon.glb`, `Wasp.glb` | Animated monsters: spider, snake, bat, dragon, wasp | Quaternius | fetched from Poly Pizza: https://poly.pizza/m/yRYJiAJyiM, https://poly.pizza/m/x9x0viZs8V, https://poly.pizza/m/hNO9XvjlKa, https://poly.pizza/m/VBvzjFIYws, https://poly.pizza/m/3aQgc75sUR |
| `cc0/quaternius/animals/Pug.fbx`, `Pig.fbx`, `Sheep.fbx` | Farm Animals Animated: pug, pig, sheep | Quaternius | https://quaternius.itch.io/lowpoly-animated-animals |
| `cc0/sigilsvault/dungeon/` | Modular Dungeon Kit v1.0: pieces and props (GLB) | Kevin Barany (SigilsVault) | https://sigilsvault.itch.io/modular-dungeon-kit-v10 |
| `cc0/unity-labs/flipbooks/` | VFX flipbooks: Flame02, Flame03, FireBall01–04, WispySmoke01, CandleSmoke01, Explosion02HD | Unity Technologies (Unity Labs Paris) | https://unity.com/blog/engine-platform/free-vfx-image-sequences-flipbooks |
| `cc0/ambientcg/` | Lava003, Rock035, Rock058 (1K, maps repacked) | ambientCG (Lennart Demes) | https://ambientcg.com |
| `cc0/texturecan/` | Volcanic Lava Flow (ground_0027), Icy Rock (ground_0031) (1K, maps repacked) | TextureCan | https://www.texturecan.com (CC0: https://www.texturecan.com/terms/) |
| `cc0/binbun/` | Hit FX, Explosion FX and Flame FX, the free versions (Godot 4 shaders, scripts and effect scenes; their `res://` paths rewritten to this folder) | Binbun3D | https://binbun3d.itch.io |
| `cc0/rpicster/` | Godot particle and VFX textures (256 px, alpha) | Raffaele Picca | https://github.com/RPicster/Godot-particle-and-vfx-textures |
| `cc0/kenney/particles/` | Particle Pack (transparent PNGs) | Kenney | https://kenney.nl/assets/particle-pack |
| `cc0/icons/flare/armor.png` | Armor Icons by Equipment Slot | Clint Bellanger, Blarumyrran, crowline, Justin Nichol | https://opengameart.org/content/armor-icons-by-equipment-slot |
| `cc0/icons/flare/weapons-2/` | Flare weapon icons 2 | Clint Bellanger | https://opengameart.org/content/flare-weapon-icons-2 |
| `cc0/icons/flare/osare/` | OSARE weapon icons | Blarumyrran | https://opengameart.org/content/osare-weapon-icons |

Textures larger than 1024 px (512 px for the SigilsVault kit) were scaled
down and re-encoded, and the flipbooks were converted from TGA to PNG at full
size; ambientCG and TextureCan maps were repacked as albedo, OpenGL normal,
AO/roughness/metal and emission; nothing else was changed.

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
