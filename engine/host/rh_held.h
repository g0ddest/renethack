/* What each monster in view was last told holding in its hand, by the
   monster's id (rh_bridge.c: "wields").  Taking up a weapon changes no
   glyph, so the bridge compares what a monster holds now with what was
   told, and draws its cell again when they differ.  Only the monsters
   still on the level are kept: before each comparison every entry is
   unmarked, the monsters there marked, the rest swept away.  No NetHack
   types: tested on its own (tests/test_held.c). */
#ifndef RH_HELD_H
#define RH_HELD_H

/* nothing held (or not told) */
#define RH_HELD_NONE (-1)
/* monsters remembered at most; past it a new one is not remembered */
#define RH_HELD_MAX 256

/* what monster `id` was last told holding, else RH_HELD_NONE */
int rh_held_get(unsigned id);
/* monster `id` told holding `glyph` (RH_HELD_NONE forgets it); marked */
void rh_held_set(unsigned id, int glyph);
/* every entry unmarked; then the monsters still there marked; then the
   unmarked ones swept away */
void rh_held_unmark(void);
void rh_held_mark(unsigned id);
void rh_held_sweep(void);
/* entries remembered now */
int rh_held_count(void);
/* everything forgotten (no game, a lost client) */
void rh_held_clear(void);

#endif
