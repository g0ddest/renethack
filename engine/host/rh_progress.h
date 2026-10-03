/* renethack engine host: what the hero's progress notice may tell.
 * Deliberately free of NetHack headers so it can be unit-tested alone;
 * rh_bridge.c checks that the achievement numbers below are NetHack's. */
#ifndef RH_PROGRESS_H
#define RH_PROGRESS_H

/* NetHack's achievements that would spoil the game while it is played
   (LL_SPOILER in insight.c's achieve_msg): picking up the gray stone that
   is Mines' End's luckstone, or Sokoban's prize, must not announce what
   it is.  #chronicle hides them until the game is over, and so do we. */
#define RH_ACH_MINE_PRIZE 10
#define RH_ACH_SOKO_PRIZE 11

/* whether achievement `ach` reaches the client now */
int rh_achievement_shown(int ach, int gameover);

/* how a game ended, from what done() leaves behind: the killer's name,
   which it sets to "quit", "escaped", "ascended", "panic" or "trickery"
   for the endings that are not deaths, whether the hero ascended, and
   whether they are alive (done() sets hit points to 0 for every death:
   a pet named "quit" that kills the hero is a death).  "died" for every
   death; NULL while the game goes on. */
const char *rh_end_how(int gameover, const char *killer_name, int ascended,
                       int alive);

#endif /* RH_PROGRESS_H */
