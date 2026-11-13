# Art credits

Everything under `cc0/` is dedicated to the public domain (CC0 1.0,
https://creativecommons.org/publicdomain/zero/1.0/). Credit is not required;
it is given here gladly. `tools/fetch_art.py` fetches these files again from
the recipe in `sources.json` and checks the downloads against `art.lock.json`.

| Where | What | Author | Source |
|---|---|---|---|
| `cc0/polyhaven/textures/` | stone tiles, castle brick, dirt floor, a rock wall, old planks, rusty metal, marble, stone walls, castle wall, monastery and rock-tile floors, volcanic rock tiles (1K) | Poly Haven artists | https://polyhaven.com/textures |
| `cc0/polyhaven/models/` | street rat, boulder, dead tree trunk, gothic statue; estoc, dagger, mace, war hammer, kite shield, three axes, oil lamp, lantern, pick; barrels, crates, treasure chest, stone fire pit; large castle door, large iron gate, lantern chandelier, wooden candlestick, brass candleholders (1K textures) | Poly Haven artists | https://polyhaven.com/models |
| `cc0/quaternius/outfits/` | Modular Character Outfits – Fantasy (Standard): peasant, ranger | Quaternius | https://quaternius.itch.io/modular-character-outfits-fantasy |
| `cc0/quaternius/animations/` | Universal Animation Library (Standard) | Quaternius | https://quaternius.itch.io/universal-animation-library |
| `cc0/quaternius/animations/UAL2_Standard.glb` | Universal Animation Library 2 (Standard) | Quaternius | https://quaternius.itch.io/universal-animation-library-2 |
| `cc0/quaternius/weapons/` | Medieval Weapons Pack: bows, arrow, spear | Quaternius | https://quaternius.itch.io/lowpoly-medieval-weapons |
| `cc0/quaternius/animals/Wolf.glb`, `Husky.glb`, `Horse.glb`, `WhiteHorse.glb`, `Cow.glb`, `Bull.glb` | Ultimate Animated Animal Pack: wolf, husky, horse, white horse, cow, bull | Quaternius | https://quaternius.com/packs/ultimateanimatedanimals.html (fetched from the Poly Pizza mirror: https://poly.pizza/m/P1gU3Qkr9r, https://poly.pizza/m/wcWiuEqwzq, https://poly.pizza/m/qvTrSG9pZF, https://poly.pizza/m/bEdE4rmZy9, https://poly.pizza/m/26zM1outCr, https://poly.pizza/m/a8PIIYwF7r) |
| `cc0/quaternius/base/` | Universal Base Characters (Standard): the base heads (face, eyes, eyebrows), hairstyles and beards | Quaternius | https://quaternius.itch.io/universal-base-characters |
| `cc0/quaternius/bestiary/` | Bestiary – Dungeon Monsters Kit (Standard): imp, puglin | Quaternius | https://quaternius.itch.io/bestiary-dungeon-monsters-kit |
| `cc0/quaternius/props/` | Fantasy Props MegaKit (Standard), a selection | Quaternius | https://quaternius.itch.io/fantasy-props-megakit |
| `cc0/quaternius/monsters/Spider.glb`, `Snake.glb`, `Bat.glb`, `Dragon.glb`, `Wasp.glb`, `Skeleton.glb` | Animated monsters: spider, snake, bat, dragon, wasp, skeleton | Quaternius | fetched from Poly Pizza: https://poly.pizza/m/yRYJiAJyiM, https://poly.pizza/m/x9x0viZs8V, https://poly.pizza/m/hNO9XvjlKa, https://poly.pizza/m/VBvzjFIYws, https://poly.pizza/m/3aQgc75sUR, https://poly.pizza/m/wODZYCgX5Z |
| `cc0/quaternius/monsters/Mimic.glb` | A mimic: a chest with an eye and teeth on long black limbs | Quaternius | fetched from Poly Pizza: https://poly.pizza/m/B8HrWzkuNp |
| `cc0/quaternius/animals/Pug.fbx`, `Pig.fbx`, `Sheep.fbx` | Farm Animals Animated: pug, pig, sheep | Quaternius | https://quaternius.itch.io/lowpoly-animated-animals |
| `cc0/made/Stethoscope.glb`, `StethoscopeMan.glb`, `StethoscopeWoman.glb` | A stethoscope (rubber tube, binaural, chest piece) built from curves: coiled on the floor, and round the neck of the peasant outfits above, fitted to them and skinned to their skeletons as the outfit under it is | this project | made here (`tools/blender/neck.py`) |
| `cc0/made/CameraMan.glb`, `CameraWoman.glb` | A camera on its strap round the neck of the peasant outfits above, resting on the breastbone, fitted to them and skinned to their skeletons as the outfit under it is | this project | made here (`tools/blender/neck.py`) |
| `cc0/made/Centaur.glb`, `CentaurFemale.glb` | Centaurs put together in Blender (`tools/blender/centaur.py`): the horse above without its neck and head, a base character's upper body (posed by the Universal Animation Library's idle) in their place, the linen painted on its skin dyed leather, the horse's body unwrapped and painted a coat streaked like hair | Quaternius | made here from the files above |
| `cc0/made/DragonRearing.glb` | The dragon above with its wings raised in every clip (`tools/gltf_turn.py`) | Quaternius | made here from the file above |
| `cc0/made/Jabberwock.glb` | The dragon above with its wings raised and its neck drawn out long (`tools/gltf_turn.py`) | Quaternius | made here from the file above |
| `cc0/made/DragonSmooth.glb`, `DragonRearingSmooth.glb`, `JabberwockSmooth.glb` | The dragons above rounded: each face cut in four twice, the new corners curved out (`tools/gltf_smooth.py`) | Quaternius | made here from the files above |
| `cc0/made/skins/` | Skins painted (`tools/art_paint.py`): a corpse's (`zombie_m`, from the base character's light skin above), a lich's dried over the skull (`lich_m`, from the same), linen wound over a body (`mummy_m` by that skin's layout, `wraps`), the Puglin's coat without its green (`puglin_pale`), the imp's without its red (`imp_pale`), the street rat's coat lit up (`rat_light`), the Ranger's and the Peasant's outfits' painting without its colours (`ranger_relief`, `peasant_relief`), faces that read at a distance (`face_m`, `face_m_shaven`, `face_m_woad`, `face_f`, `face_f_kohl`, `face_f_woad`, from the base characters' light skins above) | Quaternius, Poly Haven artists, this project | made here from the files above |
| `cc0/made/textures/do_lames/` | A Samurai's laced cuirass painted on the Ranger outfit's body by its UVs (`tools/art_paint.py`) | Quaternius, this project | made here from the files above |
| `cc0/made/textures/troll_hide/` | A troll's hide: warts and creases painted over the Puglin's own maps (`tools/art_paint.py`) | Quaternius, this project | made here from the files above |
| `cc0/made/textures/granite/` | The boulder above in grey granite: its own cracks kept, softened, speckled with granite's crystals, chips struck off it (`tools/art_paint.py`) | Poly Haven artists, this project | made here from the file above |
| `cc0/made/textures/marble/`, `marble_slabs/`, `pile/` | Polished white marble veined grey and gold, whole and laid in slabs, and a rug's pile (`tools/art_paint.py`) | this project | made here |
| `cc0/made/textures/ring_mail/`, `winingas/` | Ring mail painted on the Ranger outfit's body and sleeves by their UVs, and wool leg wraps under crossed thongs (`tools/art_paint.py`) | Quaternius, this project | made here from the files above |
| `cc0/sigilsvault/dungeon/` | Modular Dungeon Kit v1.0: pieces and props (GLB) | Kevin Barany (SigilsVault) | https://sigilsvault.itch.io/modular-dungeon-kit-v10 |
| `cc0/unity-labs/flipbooks/` | VFX flipbooks: Flame02, Flame03, FireBall01–04, WispySmoke01, CandleSmoke01, Explosion02HD | Unity Technologies (Unity Labs Paris) | https://unity.com/blog/engine-platform/free-vfx-image-sequences-flipbooks |
| `cc0/ambientcg/` | Lava003, Rock035, Rock058, Chainmail004, Metal038 (1K, maps repacked) | ambientCG (Lennart Demes) | https://ambientcg.com |
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
AO/roughness/metal and emission, and the armours' metals (Chainmail004,
Metal038) were given a brighter albedo and more roughness (the recipe's
`gain`: a true metal is black in a dim dungeon); nothing else was changed.

Godot writes an `.import` file next to every asset when it imports the
project (committed, as usual for Godot projects: they hold the import
settings, e.g. mipmaps and GPU compression for the textures) and extracts
the textures embedded in `bestiary/*.glb`, `sigilsvault/dungeon/*/*.glb` and
the models made in Blender next to them (`Imp_T_Imp_*.jpg`,
`Puglin_T_Puglin_*.jpg`, `<piece>_TrimSheet_*.jpg`, `Centaur_T_*.jpg`…). `manifest.json` says which
model, material and animation draws each monster, object and map feature.

## CC-BY

The models under `ccby/` are under the Creative Commons Attribution 3.0
license (https://creativecommons.org/licenses/by/3.0/), from Poly Pizza.
They came as static meshes, and were welded, scaled, skinned to the skeleton
of Quaternius's bull or wolf (above, CC0) and given its clips
(`tools/blender/rig_quadruped.py`); the armadillo got antennae, the
bear an owl's beak, eyes and ear tufts, the dark elephant long hair
hanging all over it (thin cards cut out of a picture of strands painted
here) and long upcurving tusks, the ground sloth's coat was painted again
in shades of blue-green, and the gorilla's four painted colours became
four plain materials; a second gorilla wears the same hair, and eyes that
glint. The scorpion and the centipede are kept as they came, unrigged. The textures were scaled down.

| Where | What | Author | Source |
|---|---|---|---|
| `ccby/polypizza/Elephant.glb` | Elephant (mumakil) | Poly by Google | https://poly.pizza/m/cx0-TiCjDOx |
| `ccby/polypizza/ElephantDark.glb` | Elephant (mastodons) | Poly by Google | https://poly.pizza/m/a27MA0rXyyj |
| `ccby/polypizza/Rhinoceros.glb` | Rhinoceros (titanotheres) | Poly by Google | https://poly.pizza/m/11qk1uy0YGH |
| `ccby/polypizza/RhinocerosDark.glb` | Rhinoceros (baluchitheria) | Poly by Google | https://poly.pizza/m/7XutktqrTj_ |
| `ccby/polypizza/GroundSloth.glb` | Ground sloth (wumpuses) | Poly by Google | https://poly.pizza/m/34WS63awSqJ |
| `ccby/polypizza/Gorilla.glb` | Gorilla (apes) | Poly by Google | https://poly.pizza/m/bmfQ1j9CeO2 |
| `ccby/polypizza/GorillaShaggy.glb` | Gorilla, under a long shag, its eyes glinting (yetis, sasquatches) | Poly by Google | https://poly.pizza/m/bmfQ1j9CeO2 |
| `ccby/polypizza/GorillaSmooth.glb`, `GorillaShaggySmooth.glb` | The two gorillas above rounded, the shag left as it is: each face of the body cut in four twice, the new corners curved out (`tools/gltf_smooth.py`) | Poly by Google | made here from the files above |
| `ccby/polypizza/Baboon.glb` | Baboon (monkeys) | Poly by Google | https://poly.pizza/m/aeLFKp6X19x |
| `ccby/polypizza/Bear.glb` | Bear (owlbears) | Poly by Google | https://poly.pizza/m/0PXWfxfb0Hu |
| `ccby/polypizza/Hyena.glb` | Spotted hyena (leocrottas) | Poly by Google | https://poly.pizza/m/0yU1LU3Nkpu |
| `ccby/polypizza/Armadillo.glb` | Armadillo (rust monsters, disenchanters) | Poly by Google | https://poly.pizza/m/dZ3EWgIsSJQ |
| `ccby/polypizza/Scorpion.glb` | Scorpion (scorpions, Scorpius) | Poly by Google | https://poly.pizza/m/6Bu7d_Pkm5o |
| `ccby/polypizza/Centipede.glb` | Centipede (centipedes) | Poly by Google | https://poly.pizza/m/8C36B7L_2hR |

## Icons of spells and states

The symbols of the icons of the spells and of the hero's states come from
game-icons.net and are under the
Creative Commons Attribution 3.0 license
(https://creativecommons.org/licenses/by/3.0/), Zeromancer's heart under
CC0 1.0 (https://creativecommons.org/publicdomain/zero/1.0/). Each was
painted here on its plate, a spell's school's or a state's tone's (`make
spell-icons`, `make state-icons`): lit as a relief,
given a glow, an outline and a shadow, and scaled down.

| Where | What | Author | Source |
|---|---|---|---|
| `ccby/game-icons/lorc/dig-dug.svg` | Dig dug (dig) | Lorc | https://game-icons.net/1x1/lorc/dig-dug.html |
| `ccby/game-icons/lorc/energy-arrow.svg` | Energy arrow (magic missile) | Lorc | https://game-icons.net/1x1/lorc/energy-arrow.html |
| `ccby/game-icons/lorc/burning-meteor.svg` | Burning meteor (fireball) | Lorc | https://game-icons.net/1x1/lorc/burning-meteor.html |
| `ccby/game-icons/lorc/energy-breath.svg` | Energy breath (cone of cold) | Lorc | https://game-icons.net/1x1/lorc/energy-breath.html |
| `ccby/game-icons/delapouite/night-sleep.svg` | Night sleep (sleep) | Delapouite | https://game-icons.net/1x1/delapouite/night-sleep.html |
| `ccby/game-icons/lorc/death-zone.svg` | Death zone (finger of death) | Lorc | https://game-icons.net/1x1/lorc/death-zone.html |
| `ccby/game-icons/lorc/sun.svg` | Sun (light) | Lorc | https://game-icons.net/1x1/lorc/sun.html |
| `ccby/game-icons/lorc/beast-eye.svg` | Beast eye (detect monsters) | Lorc | https://game-icons.net/1x1/lorc/beast-eye.html |
| `ccby/game-icons/zeromancer/heart-plus.svg` | Heart plus (healing) | Zeromancer (CC0) | https://game-icons.net/1x1/zeromancer/heart-plus.html |
| `ccby/game-icons/lorc/unlocking.svg` | Unlocking (knock) | Lorc | https://game-icons.net/1x1/lorc/unlocking.html |
| `ccby/game-icons/lorc/punch-blast.svg` | Punch blast (force bolt) | Lorc | https://game-icons.net/1x1/lorc/punch-blast.html |
| `ccby/game-icons/delapouite/knocked-out-stars.svg` | Knocked out stars (confuse monster) | Delapouite | https://game-icons.net/1x1/delapouite/knocked-out-stars.html |
| `ccby/game-icons/delapouite/blindfold.svg` | Blindfold (cure blindness) | Delapouite | https://game-icons.net/1x1/delapouite/blindfold.html |
| `ccby/game-icons/lorc/life-tap.svg` | Life tap (drain life) | Lorc | https://game-icons.net/1x1/lorc/life-tap.html |
| `ccby/game-icons/lorc/snail.svg` | Snail (slow monster) | Lorc | https://game-icons.net/1x1/lorc/snail.html |
| `ccby/game-icons/lorc/padlock.svg` | Padlock (wizard lock) | Lorc | https://game-icons.net/1x1/lorc/padlock.html |
| `ccby/game-icons/lorc/imp-laugh.svg` | Imp laugh (create monster) | Lorc | https://game-icons.net/1x1/lorc/imp-laugh.html |
| `ccby/game-icons/lorc/meat.svg` | Meat (detect food) | Lorc | https://game-icons.net/1x1/lorc/meat.html |
| `ccby/game-icons/lorc/terror.svg` | Terror (cause fear) | Lorc | https://game-icons.net/1x1/lorc/terror.html |
| `ccby/game-icons/lorc/crystal-ball.svg` | Crystal ball (clairvoyance) | Lorc | https://game-icons.net/1x1/lorc/crystal-ball.html |
| `ccby/game-icons/delapouite/medicines.svg` | Medicines (cure sickness) | Delapouite | https://game-icons.net/1x1/delapouite/medicines.html |
| `ccby/game-icons/lorc/puppet.svg` | Puppet (charm monster) | Lorc | https://game-icons.net/1x1/lorc/puppet.html |
| `ccby/game-icons/lorc/sprint.svg` | Sprint (haste self) | Lorc | https://game-icons.net/1x1/lorc/sprint.html |
| `ccby/game-icons/delapouite/secret-door.svg` | Secret door (detect unseen) | Delapouite | https://game-icons.net/1x1/delapouite/secret-door.html |
| `ccby/game-icons/lorc/feather.svg` | Feather (levitation) | Lorc | https://game-icons.net/1x1/lorc/feather.html |
| `ccby/game-icons/sbed/health-increase.svg` | Health increase (extra healing) | Sbed | https://game-icons.net/1x1/sbed/health-increase.html |
| `ccby/game-icons/lorc/muscle-up.svg` | Muscle up (restore ability) | Lorc | https://game-icons.net/1x1/lorc/muscle-up.html |
| `ccby/game-icons/delapouite/invisible.svg` | Invisible (invisibility) | Delapouite | https://game-icons.net/1x1/delapouite/invisible.html |
| `ccby/game-icons/skoll/open-treasure-chest.svg` | Open treasure chest (detect treasure) | Skoll | https://game-icons.net/1x1/skoll/open-treasure-chest.html |
| `ccby/game-icons/skoll/breaking-chain.svg` | Breaking chain (remove curse) | Skoll | https://game-icons.net/1x1/skoll/breaking-chain.html |
| `ccby/game-icons/lorc/treasure-map.svg` | Treasure map (magic mapping) | Lorc | https://game-icons.net/1x1/lorc/treasure-map.html |
| `ccby/game-icons/lorc/magnifying-glass.svg` | Magnifying glass (identify) | Lorc | https://game-icons.net/1x1/lorc/magnifying-glass.html |
| `ccby/game-icons/delapouite/shambling-zombie.svg` | Shambling zombie (turn undead) | Delapouite | https://game-icons.net/1x1/delapouite/shambling-zombie.html |
| `ccby/game-icons/lorc/frog.svg` | Frog (polymorph) | Lorc | https://game-icons.net/1x1/lorc/frog.html |
| `ccby/game-icons/lorc/teleport.svg` | Teleport (teleport away) | Lorc | https://game-icons.net/1x1/lorc/teleport.html |
| `ccby/game-icons/lorc/owl.svg` | Owl (create familiar) | Lorc | https://game-icons.net/1x1/lorc/owl.html |
| `ccby/game-icons/lorc/interdiction.svg` | Interdiction (cancellation) | Lorc | https://game-icons.net/1x1/lorc/interdiction.html |
| `ccby/game-icons/lorc/magic-shield.svg` | Magic shield (protection) | Lorc | https://game-icons.net/1x1/lorc/magic-shield.html |
| `ccby/game-icons/delapouite/jump-across.svg` | Jump across (jumping) | Delapouite | https://game-icons.net/1x1/delapouite/jump-across.html |
| `ccby/game-icons/delapouite/stone-bust.svg` | Stone bust (stone to flesh) | Delapouite | https://game-icons.net/1x1/delapouite/stone-bust.html |
| `ccby/game-icons/lorc/lightning-branches.svg` | Lightning branches (chain lightning) | Lorc | https://game-icons.net/1x1/lorc/lightning-branches.html |
| `ccby/game-icons/lorc/ball-glow.svg` | Ball glow (flame sphere) | Lorc | https://game-icons.net/1x1/lorc/ball-glow.html |
| `ccby/game-icons/lorc/frozen-orb.svg` | Frozen orb (freeze sphere) | Lorc | https://game-icons.net/1x1/lorc/frozen-orb.html |
| `ccby/game-icons/skoll/fat.svg` | Fat (Satiated) | Skoll | https://game-icons.net/1x1/skoll/fat.html |
| `ccby/game-icons/delapouite/fork-knife-spoon.svg` | Fork knife spoon (Hungry, Weak, Fainting, Fainted) | Delapouite | https://game-icons.net/1x1/delapouite/fork-knife-spoon.html |
| `ccby/game-icons/delapouite/weight.svg` | Weight (Burdened, Stressed, Strained, Overtaxed, Overloaded) | Delapouite | https://game-icons.net/1x1/delapouite/weight.html |
| `ccby/game-icons/lorc/stoned-skull.svg` | Stoned skull (Stone) | Lorc | https://game-icons.net/1x1/lorc/stoned-skull.html |
| `ccby/game-icons/delapouite/slime.svg` | Slime (Slime) | Delapouite | https://game-icons.net/1x1/delapouite/slime.html |
| `ccby/game-icons/delapouite/lungs.svg` | Lungs (Strngl) | Delapouite | https://game-icons.net/1x1/delapouite/lungs.html |
| `ccby/game-icons/lorc/vomiting.svg` | Vomiting (FoodPois) | Lorc | https://game-icons.net/1x1/lorc/vomiting.html |
| `ccby/game-icons/delapouite/plague-doctor-profile.svg` | Plague doctor profile (TermIll) | Delapouite | https://game-icons.net/1x1/delapouite/plague-doctor-profile.html |
| `ccby/game-icons/sbed/lava.svg` | Lava (InLava) | Sbed | https://game-icons.net/1x1/sbed/lava.html |
| `ccby/game-icons/lorc/shadow-grasp.svg` | Shadow grasp (Grab) | Lorc | https://game-icons.net/1x1/lorc/shadow-grasp.html |
| `ccby/game-icons/delapouite/frozen-body.svg` | Frozen body (Parlyz) | Delapouite | https://game-icons.net/1x1/delapouite/frozen-body.html |
| `ccby/game-icons/delapouite/dead-head.svg` | Dead head (Out) | Delapouite | https://game-icons.net/1x1/delapouite/dead-head.html |
| `ccby/game-icons/lorc/sleepy.svg` | Sleepy (Zzz) | Lorc | https://game-icons.net/1x1/lorc/sleepy.html |
| `ccby/game-icons/skoll/sight-disabled.svg` | Sight disabled (Blind) | Skoll | https://game-icons.net/1x1/skoll/sight-disabled.html |
| `ccby/game-icons/lorc/swirl-string.svg` | Swirl string (Conf) | Lorc | https://game-icons.net/1x1/lorc/swirl-string.html |
| `ccby/game-icons/skoll/knockout.svg` | Knockout (Stun) | Skoll | https://game-icons.net/1x1/skoll/knockout.html |
| `ccby/game-icons/lorc/spotted-mushroom.svg` | Spotted mushroom (Hallu) | Lorc | https://game-icons.net/1x1/lorc/spotted-mushroom.html |
| `ccby/game-icons/skoll/hearing-disabled.svg` | Hearing disabled (Deaf) | Skoll | https://game-icons.net/1x1/skoll/hearing-disabled.html |
| `ccby/game-icons/lorc/manacles.svg` | Manacles (Held) | Lorc | https://game-icons.net/1x1/lorc/manacles.html |
| `ccby/game-icons/lorc/wolf-trap.svg` | Wolf trap (Trap) | Lorc | https://game-icons.net/1x1/lorc/wolf-trap.html |
| `ccby/game-icons/lorc/broken-bone.svg` | Broken bone (WLegs) | Lorc | https://game-icons.net/1x1/lorc/broken-bone.html |
| `ccby/game-icons/lorc/drowning.svg` | Drowning (Submrg) | Lorc | https://game-icons.net/1x1/lorc/drowning.html |
| `ccby/game-icons/lorc/harpoon-chain.svg` | Harpoon chain (Teth) | Lorc | https://game-icons.net/1x1/lorc/harpoon-chain.html |
| `ccby/game-icons/delapouite/oil-can.svg` | Oil can (Slip) | Delapouite | https://game-icons.net/1x1/delapouite/oil-can.html |
| `ccby/game-icons/delapouite/air-balloon.svg` | Air balloon (Lev) | Delapouite | https://game-icons.net/1x1/delapouite/air-balloon.html |
| `ccby/game-icons/lorc/feathered-wing.svg` | Feathered wing (Fly) | Lorc | https://game-icons.net/1x1/lorc/feathered-wing.html |
| `ccby/game-icons/lorc/horse-head.svg` | Horse head (Ride) | Lorc | https://game-icons.net/1x1/lorc/horse-head.html |
| `ccby/game-icons/lorc/glowing-hands.svg` | Glowing hands (Glow) | Lorc | https://game-icons.net/1x1/lorc/glowing-hands.html |
| `ccby/game-icons/delapouite/ice-skate.svg` | Ice skate (Icy) | Delapouite | https://game-icons.net/1x1/delapouite/ice-skate.html |
| `ccby/game-icons/lorc/grab.svg` | Grab (UHold) | Lorc | https://game-icons.net/1x1/lorc/grab.html |
| `ccby/game-icons/lorc/fist.svg` | Fist (Bare) | Lorc | https://game-icons.net/1x1/lorc/fist.html |
| `ccby/game-icons/lorc/hourglass.svg` | Hourglass (Busy) | Lorc | https://game-icons.net/1x1/lorc/hourglass.html |

## Generated for this project

The models under `owner/` were made for this game with Meshy, on its
owner's account: a concept picture, then a textured mesh. Their 4K maps
were cut to 2K and written as JPEGs beside a GLB that holds the geometry and
names them, with a map of what glows taken from the albedo's fiery or red
texels where a figure glows (`*_glow.jpg`); nothing else was changed.

| Where | What |
|---|---|
| `owner/prologue/marduk.glb` | Marduk the Creator: a god-king in a tiered horned crown, ivory and gold robes, a rod and a ring of rule (62k triangles) |
| `owner/prologue/moloch.glb` | Moloch: a bull-headed dark god, a furnace in his chest, chains, a torn cape (62k triangles) |
| `owner/prologue/amulet.glb` | The Amulet of Yendor: tarnished gold, rings of glyphs, a red stone in claws, a short chain (30k triangles) |
| `owner/prologue/statue.glb` | A nameless god: a towering hooded colossus of weathered basalt on a plinth, its cowl black within, hands on a staff (40k triangles) |

The bestiary's and the heroes' generated bodies were made the same way and
rigged by Meshy (the black dragon on bones of our own). Their maps were cut
to 1K (the Valkyrie's to 2K); some albedos were dyed for the game's dark
light, and the Mines folk's coloured ones are Meshy retextures. Bodies that
wear our kits were re-rigged to our skeleton and clips.

| Where | What |
|---|---|
| `owner/dragon_black.glb` | A black dragon, wings spread (31k triangles) |
| `owner/lich2.glb` | A robed lich, its robe dyed in each rank's colour (31k triangles) |
| `owner/demon.glb` | A horned demon, the balrog and the pit fiend (31k triangles) |
| `owner/devil.glb`, `devil_ice.jpg`, `devil_bone.jpg` | A lean devil: horned, barbed, ice and bone (31k triangles) |
| `owner/flayer.glb` | A mind flayer (31k triangles) |
| `owner/vampire.glb` | A caped vampire lord (31k triangles) |
| `owner/troll.glb` | A troll, its skin dyed in each kind's colour (31k triangles) |
| `owner/giant.glb` | A hill giant with a skull belt and a fur mantle (31k triangles) |
| `owner/dwarf.glb`, `dwarf_red.jpg`, `dwarf_blue.jpg`, `dwarf_magenta.jpg` | An armoured dwarf, in his rank's colours (21k triangles) |
| `owner/gnome.glb`, `gnome_brown.jpg`, `gnome_blue.jpg`, `gnome_magenta.jpg`, `gnome_wizard.jpg` | A hooded gnome, in his rank's colours (21k triangles) |
| `owner/valkyrie.glb` | The owner's woman warrior, the female Valkyrie: cloth wraps, a long split coat (41k triangles) |
| `owner/dog.glb` | A wiry grey hound: the little dog, the dog and the large dog, its weights taken from Quaternius' husky so that the husky's clips play on it (12k triangles) |

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
