# The logo

The game has no logo yet. Until an artist letters one, its name is set
in type in two places:

- **The title screen** (`title_name` in `client/rust/renethack-gd/src/screens.rs`)
  sets "RENETHACK" in Cormorant SC SemiBold at 88 px. `ui/ui_title.gdshader`
  strikes the letters in gold: bright at their tops, a polished band
  across their middle, dark at their feet, each stroke's upper edge lit
  and its lower edge shaded. It has a dark outline and a soft shadow,
  and a gold flourish under it (a diamond between two tapering lines).
- **The Steam art** (`store_art.rs`, `make steam-art`; see
  [steam.md](steam.md)) sets the name in plain gold in the same face on
  the capsules and on `library_logo.png`.

This is a placeholder. A real logo needs an artist.

## What to ask an artist for

**The wordmark.** "RENETHACK", lettered by hand rather than set in a
stock font:

- Dark-fantasy small capitals, in the family of Cormorant SC, which the
  rest of the UI keeps for its titles.
- It must read in the small capsule (462×174) as well as on the
  library's logo (1280×157).
- A version with a mark (below) and a version without it.

**The material.** Cast or forged metal, matching the HUD:

- gold to bronze, with real bevels, edge highlights and a little wear;
- room for one accent, such as embers in the recesses or a faint inner
  glow, as long as it doesn't fight the torchlight of the scenes it
  sits over.

The HUD's colours are in `theme.rs`: `GOLD` #b8893b, `GOLD_BRIGHT`
#e7c27a, `GOLD_DIM` #6e5328, iron #3a2f25. The title's backdrop is dark
and warm, about #0b0908.

**A mark.** A small emblem that works without the word. It could build
on NetHack's own heritage: the `@` of the hero, or the shield and
crossed swords of the app icon (`tools/app_icon.py`). It would serve
the app icon, the start-up veil and the capsules.

**Deliverables:**

- the layered source;
- PNGs with alpha, the wordmark 2048 px wide and the mark 1024 px
  square;
- `library_logo.png` at 1280×157 on a clear background;
- a one-colour version, for embossing and for small sizes.

## Fitting it

1. Put the PNGs in `client/godot/ui/` and import them.
2. Change `title_name` to show the wordmark in a TextureRect about
   440 px wide, which is the buttons' width, and drop the flourish if
   the logo carries its own.
3. In `store_art.rs`, draw the wordmark where the name is set now, and
   take `library_logo.png` out of `SHOTS` once it holds the artist's
   file.

The type and the shader can then go.
